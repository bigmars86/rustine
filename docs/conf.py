# Gelatin (Rust) Documentation — Sphinx configuration
import os
import datetime

project = "Rustine"
author = "Rustine Contributors"
copyright = f"{datetime.datetime.now().year}, {author}"

# -- General -----------------------------------------------------------
extensions = [
    "myst_parser",          # Markdown support (.md files)
    "sphinx.ext.autodoc",
    "sphinx.ext.viewcode",
]

source_suffix = {
    ".rst": "restructuredtext",
    ".md": "markdown",
}

master_doc = "index"

# -- Theme -------------------------------------------------------------
html_theme = "sphinx_rtd_theme"
html_theme_options = {
    "navigation_depth": 3,
}

# -- myst-parser -------------------------------------------------------
myst_enable_extensions = [
    "colon_fence",
    "deflist",
]
