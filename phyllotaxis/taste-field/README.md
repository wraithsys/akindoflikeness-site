# The taste field, v0 (2026-08-21)

First fitted model of Billy's harmonic taste, from 132 rated passages
across seven tasting passes (harmony-tastings.html corpus). Ridge
regression over 16 passage-level features, leave-one-out validated.

**LOO: 85% within ±1 (his own test–retest noise floor is ±1), mean
|error| 0.93, vs 74% for predict-the-mean and 71–75% for Claude's
sealed informal predictions on decks 3–4. The fitted field beats both.**

Weights in field-v0-report.txt. Headlines: held voices help and
scattered motion hurts as SEPARATE forces (the deck-2 "tissue is not
the variable" was a confound); soprano return is the third-strongest
positive (the melodic-line term his M01 word demanded); plain major
weighted low-not-zero exactly per his instruction; semitone bass
motion strongly positive; destination-less returns negative.

Known limits, honestly: the colour feature is a crude count that
can't tell his palette from mud (comes out slightly negative —
metric fault, not taste fault); wholetone_bass and late_spike ride
on too few cards to trust. Next iteration needs a real palette
feature and roughness from the phyllotaxis dissonance model.
