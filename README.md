# akindoflikeness.net

The AKOL / AKOL (instruments) landing page. Static, single self-contained
`index.html` — fonts, the datamoshed hero loop and imagery are inlined, so
there is nothing else to serve.

Deployed on Cloudflare Pages, connected to this repo: every push to `main`
redeploys. Custom domain `akindoflikeness.net`.

Source of truth for the page is `perflab/site/akol-net.template.html` in the
main dev tree; `index.html` here is the built, domain-corrected output.

TODO: un-inline assets (fonts/video/images as real files) so updates don't
rewrite a 2.5MB HTML and the browser can cache them. Fine as one file for now.
