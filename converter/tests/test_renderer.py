from md2kindle.renderer import DEFAULT_CSS, render_markdown


def test_renders_headings_and_emphasis():
    html = render_markdown("# Title\n\nSome *bold* text\n")
    assert "<h1" in html and "id=\"title\"" in html
    assert "<em>bold</em>" in html


def test_includes_css_and_doctype():
    html = render_markdown("# Hi\n")
    assert html.startswith("<!DOCTYPE html>")
    assert "<style>" in html and DEFAULT_CSS in html


def test_renders_image_tag():
    # Attribute order in <img> is python-markdown-dependent, so check each attr independently.
    html = render_markdown("![alt](photo.png)\n")
    assert 'src="photo.png"' in html
    assert 'alt="alt"' in html
