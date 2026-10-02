"""Subsets the site's two typefaces to what its five languages need, into public/fonts/ and scripts/assets/fonts/.

The sources are the @fontsource devDependencies, so run `npm ci` first. Needs `pip install fonttools brotli`. Both fonts
are under the SIL Open Font License (public/fonts/OFL-*.txt). The output is committed and a normal build does not run this.

    python3 scripts/subset-fonts.py
"""

from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "public" / "fonts"
ASSETS = ROOT / "scripts" / "assets" / "fonts"

# Basic Latin and Latin-1 (every accented letter of fr, it, es, de), the oe ligature (Instrument Sans has none, the
# browser falls back for it), typographic quotes and dashes, the ellipsis, a bullet, the minus sign and the euro.
UNICODES = (
    list(range(0x20, 0x7F))
    + list(range(0xA0, 0x100))
    + [0x152, 0x153, 0x2013, 0x2014, 0x2018, 0x2019, 0x201A, 0x201C, 0x201D, 0x201E, 0x2022, 0x2026, 0x2212, 0x20AC]
)

# fontsource splits each font by script; its "latin" file already holds Latin-1.
INSTRUMENT = ROOT / "node_modules/@fontsource-variable/instrument-sans/files/instrument-sans-latin-wght-normal.woff2"
PLEX = ROOT / "node_modules/@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-{weight}-normal.woff2"


def subset_font(font: TTFont, name: str) -> None:
    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = ["kern", "liga", "calt", "ccmp", "locl", "mark", "mkmk", "zero"]
    options.drop_tables += ["DSIG"]
    options.name_IDs = [1, 2, 3, 4, 6]
    options.notdef_outline = True
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=UNICODES)
    subsetter.subset(font)
    font.flavor = "woff2"
    font.save(str(OUT / name))
    print(name, (OUT / name).stat().st_size)


# Instrument Sans is variable in weight (400 to 700) and the site uses the whole axis.
subset_font(TTFont(str(INSTRUMENT)), "instrument-sans-var.woff2")

for weight in (400, 500):
    subset_font(TTFont(str(PLEX).format(weight=weight)), f"ibm-plex-mono-{weight}.woff2")

# scripts/build-images.mjs draws the social image's text with opentype.js, which reads TrueType, not WOFF2: write the
# semibold (640) instance of Instrument Sans and the regular Plex Mono as TTF in scripts/assets/fonts. Only kerning is
# kept in these copies, the one lookup opentype.js applies.
ASSETS.mkdir(parents=True, exist_ok=True)
heading = instancer.instantiateVariableFont(TTFont(str(OUT / "instrument-sans-var.woff2")), {"wght": 640})
kern_only = subset.Options()
kern_only.layout_features = ["kern"]
subsetter = subset.Subsetter(kern_only)
subsetter.populate(unicodes=UNICODES)
subsetter.subset(heading)
heading.flavor = None
heading.save(str(ASSETS / "instrument-sans-640.ttf"))
mono = TTFont(str(OUT / "ibm-plex-mono-400.woff2"))
mono.flavor = None
mono.save(str(ASSETS / "ibm-plex-mono-400.ttf"))
