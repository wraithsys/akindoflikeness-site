//! Chat-to-instrument relay for the streaming rig.
//!
//! One end reads a stream chat (Twitch natively; anything else piped in on
//! stdin). The other end is a one-route HTTP server on loopback that the
//! instrument page, opened with `?live`, polls every couple of seconds. In
//! between sits the only policy this tool has:
//!
//! - **A patch is a run of `id:value` pairs** — the same format the page's
//!   SHARE button puts in the URL fragment. It is recognised anywhere in a
//!   message, with or without the URL around it, because YouTube chat likes to
//!   eat links and viewers will paste the bare fragment.
//! - **Latest wins, but a patch holds the floor.** A newly arrived patch does
//!   not interrupt the current one until `--hold` seconds have passed; while
//!   the floor is held, later patches replace the waiting one rather than
//!   queueing. A constant stream should morph, not thrash.
//! - **Nothing else is interpreted.** The relay never evaluates values or
//!   touches the engine; the page applies patches through its own sliders,
//!   which clamp every number to the range the slider already has.

use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "phyllotaxis-live", version, about)]
struct Args {
    #[command(subcommand)]
    source: Source,

    /// Loopback port the ?live page polls (matches ?live=<port>)
    #[arg(long, default_value_t = 7341)]
    port: u16,

    /// Seconds a patch keeps the instrument before the next may take it
    #[arg(long, default_value_t = 30)]
    hold: u64,
}

#[derive(Subcommand)]
enum Source {
    /// Read a Twitch channel's chat anonymously (no account, no token)
    Twitch { channel: String },
    /// Read chat lines from stdin: `username: message`, or bare messages.
    /// This is the adapter for everything that is not Twitch, e.g. YouTube:
    ///   chat_downloader "https://youtube.com/watch?v=..." | phyllotaxis-live stdin
    Stdin,
}

struct ChatLine {
    from: String,
    text: String,
}

/// What the page polls for. `seq` only ever moves forward, so the page can
/// tell a new patch from the one it already applied.
#[derive(Default)]
struct Current {
    seq: u64,
    hash: String,
    from: String,
}

fn main() {
    let args = Args::parse();

    let state = Arc::new(Mutex::new(Current::default()));
    {
        let state = state.clone();
        let port = args.port;
        std::thread::spawn(move || serve(state, port));
    }

    let (tx, rx) = mpsc::channel::<ChatLine>();
    match args.source {
        Source::Twitch { channel } => {
            let channel = channel.trim_start_matches('#').to_ascii_lowercase();
            std::thread::spawn(move || twitch(&channel, tx));
        }
        Source::Stdin => {
            std::thread::spawn(move || stdin_lines(tx));
        }
    }

    let hold = Duration::from_secs(args.hold);
    let mut last_applied: Option<Instant> = None;
    // The patch waiting for the floor. Later arrivals overwrite it: latest
    // wins, nothing queues up to be played long after its sender left.
    let mut pending: Option<(String, String)> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                if let Some(hash) = extract_patch(&line.text) {
                    let from = clean_name(&line.from);
                    println!("heard  \u{25c2} {from}  {hash}");
                    pending = Some((from, hash));
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("chat source ended");
                break;
            }
        }

        let floor_open = last_applied.is_none_or(|t| t.elapsed() >= hold);
        if floor_open {
            if let Some((from, hash)) = pending.take() {
                let mut cur = state.lock().unwrap();
                cur.seq += 1;
                cur.from = from;
                cur.hash = hash;
                println!("applied \u{25c2} {}  (seq {})", cur.from, cur.seq);
                last_applied = Some(Instant::now());
            }
        }
    }
}

/* ── patch extraction ───────────────────────────────────────────────── */

/// Longest run of `id:value` pairs found anywhere in the message, or None.
///
/// At least four pairs: a real patch carries ~16, while "see you at 1:30,
/// 2:45 works too" parses as two. Values may be negative and fractional;
/// ids are small integers. The span is returned verbatim — the page's own
/// parser is the authority on what the pairs mean.
fn extract_patch(msg: &str) -> Option<String> {
    let b = msg.as_bytes();
    let mut best: Option<&str> = None;
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let (end, pairs) = run_of_pairs(b, i);
            if pairs >= 4 && end - i <= 400 && best.is_none_or(|s| s.len() < end - i) {
                best = Some(&msg[i..end]);
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    best.map(str::to_owned)
}

/// Parse `id:value(,id:value)*` starting at `start`; return (end, pair count).
fn run_of_pairs(b: &[u8], start: usize) -> (usize, usize) {
    let mut i = start;
    let mut pairs = 0;
    let mut end = start;
    loop {
        let id_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        // Two digits cover params 0..=20 with room; more is a timestamp or
        // an ID of something else entirely.
        if i == id_start || i - id_start > 2 || i >= b.len() || b[i] != b':' {
            break;
        }
        i += 1; // ':'
        let val_start = i;
        if i < b.len() && b[i] == b'-' {
            i += 1;
        }
        let mut digits = false;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            digits |= b[i].is_ascii_digit();
            i += 1;
        }
        if !digits {
            break;
        }
        let _ = val_start;
        pairs += 1;
        end = i;
        if i < b.len() && b[i] == b',' {
            i += 1;
        } else {
            break;
        }
    }
    (end, pairs)
}

/// Chat names go on stream: keep them to the character set both Twitch and
/// sanity allow, and short.
fn clean_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(25)
        .collect();
    if s.is_empty() {
        s.push_str("chat");
    }
    s
}

/* ── sources ────────────────────────────────────────────────────────── */

fn stdin_lines(tx: mpsc::Sender<ChatLine>) {
    let stdin = std::io::stdin();
    let mut buf = String::new();
    loop {
        buf.clear();
        match stdin.read_line(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }
        // chat-downloader prints `[h:mm:ss] user: message`; take the name if
        // the shape is there, otherwise the whole line is the message.
        let line = match (line.starts_with('['), line.find(']')) {
            (true, Some(p)) => line[p + 1..].trim(),
            _ => line,
        };
        let (from, text) = match line.split_once(": ") {
            Some((u, t)) if !u.is_empty() && u.len() <= 40 => (u, t),
            _ => ("chat", line),
        };
        if tx
            .send(ChatLine { from: from.into(), text: text.into() })
            .is_err()
        {
            return;
        }
    }
}

fn twitch(channel: &str, tx: mpsc::Sender<ChatLine>) {
    loop {
        match twitch_once(channel, &tx) {
            Ok(()) => eprintln!("twitch: connection closed; reconnecting in 5s"),
            Err(e) => eprintln!("twitch: {e}; reconnecting in 5s"),
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

fn twitch_once(channel: &str, tx: &mpsc::Sender<ChatLine>) -> std::io::Result<()> {
    let sock = TcpStream::connect(("irc.chat.twitch.tv", 6697))?;
    sock.set_read_timeout(Some(Duration::from_secs(360)))?; // Twitch PINGs ~5min

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.into(),
    };
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from("irc.chat.twitch.tv")
        .expect("static hostname");
    let conn = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(std::io::Error::other)?;
    let mut tls = rustls::StreamOwned::new(conn, sock);

    // `justinfan<digits>` is Twitch's documented anonymous, read-only login.
    write!(tls, "NICK justinfan61803\r\nJOIN #{channel}\r\n")?;
    eprintln!("twitch: joined #{channel}");

    let mut buf = [0u8; 4096];
    let mut acc = String::new();
    loop {
        let n = tls.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        acc.push_str(&String::from_utf8_lossy(&buf[..n]));
        while let Some(pos) = acc.find("\r\n") {
            let line = acc[..pos].to_owned();
            acc.drain(..pos + 2);
            if let Some(rest) = line.strip_prefix("PING") {
                write!(tls, "PONG{rest}\r\n")?;
            } else if let Some((from, text)) = parse_privmsg(&line) {
                let _ = tx.send(ChatLine { from, text });
            }
        }
    }
}

/// `:nick!user@host PRIVMSG #channel :message` → (nick, message)
fn parse_privmsg(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix(':')?;
    let (prefix, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix("PRIVMSG ")?;
    let (_target, text) = rest.split_once(" :")?;
    let nick = prefix.split('!').next()?;
    Some((nick.to_owned(), text.to_owned()))
}

/* ── the loopback end ───────────────────────────────────────────────── */

fn serve(state: Arc<Mutex<Current>>, port: u16) {
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind 127.0.0.1:{port}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("serving http://127.0.0.1:{port}/patch — open the instrument with ?live");
    for stream in listener.incoming() {
        // Requests are one short GET from one local browser; serving them in
        // sequence is plenty, and a bad one only costs its read timeout.
        if let Ok(s) = stream {
            let _ = handle(s, &state);
        }
    }
}

fn handle(mut s: TcpStream, state: &Arc<Mutex<Current>>) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut head = [0u8; 1024];
    let n = s.read(&mut head)?;
    let req = String::from_utf8_lossy(&head[..n]);
    let line = req.lines().next().unwrap_or("");

    // The page is https and cross-origin isolated; without these two headers
    // the browser drops the response before the page ever sees it.
    const CORS: &str = "Access-Control-Allow-Origin: *\r\n\
                        Cross-Origin-Resource-Policy: cross-origin\r\n\
                        Cache-Control: no-store\r\n";

    let (status, body) = if line.starts_with("GET /patch") {
        let cur = state.lock().unwrap();
        // `from` and `hash` are filtered to charsets with nothing to escape,
        // so the JSON can be assembled by hand.
        (
            "200 OK",
            format!(
                r#"{{"seq":{},"hash":"{}","from":"{}"}}"#,
                cur.seq, cur.hash, cur.from
            ),
        )
    } else if line.starts_with("OPTIONS") {
        ("204 No Content", String::new())
    } else {
        ("404 Not Found", String::new())
    };

    write!(
        s,
        "HTTP/1.1 {status}\r\n{CORS}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_url_yields_its_patch() {
        let msg = "try this one https://akindoflikeness.net/instruments/phyllotaxis/#0:1,1:4.000,18:110.0,2:1.618 so good";
        assert_eq!(
            extract_patch(msg).as_deref(),
            Some("0:1,1:4.000,18:110.0,2:1.618")
        );
    }

    #[test]
    fn a_bare_fragment_survives_a_link_eating_chat() {
        let msg = "0:3,1:7.550,18:55.0,2:0.910,20:0.750";
        assert_eq!(extract_patch(msg).as_deref(), Some(msg));
    }

    #[test]
    fn timestamps_and_scores_are_not_patches() {
        assert_eq!(extract_patch("see you at 1:30"), None);
        assert_eq!(extract_patch("it went 1:30, 2:45 then 3:15"), None);
        assert_eq!(extract_patch("plain chatter"), None);
    }

    #[test]
    fn negative_values_belong_to_the_strum() {
        let msg = "#0:1,4:-0.700,5:0.900,7:0.400";
        assert_eq!(extract_patch(msg).as_deref(), Some("0:1,4:-0.700,5:0.900,7:0.400"));
    }

    #[test]
    fn privmsg_parses_to_nick_and_text() {
        let line = ":anna!anna@anna.tmi.twitch.tv PRIVMSG #akol :here 0:1,1:2.0";
        assert_eq!(
            parse_privmsg(line),
            Some(("anna".into(), "here 0:1,1:2.0".into()))
        );
    }
}
