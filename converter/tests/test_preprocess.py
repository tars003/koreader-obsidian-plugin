from md2kindle.preprocess import preprocess, strip_frontmatter
from md2kindle.resolver import build_index


IDX = build_index(["Notes/idea.md", "assets/photo.png"])


def test_strip_frontmatter():
    md = "---\ntitle: x\n---\n# Body\n"
    assert strip_frontmatter(md) == "# Body\n"


def test_no_frontmatter_untouched():
    assert strip_frontmatter("# Hi\n") == "# Hi\n"


def test_wikilink_rewritten():
    out = preprocess("See [[idea]] please", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "See [idea](idea.html) please"


def test_broken_link_renders_dimmed_span():
    out = preprocess("See [[ghost]]", "Notes/other.md", IDX, log=lambda *_: None)
    assert '<span class="broken-link">ghost</span>' in out


def test_wikilink_not_rewritten_inside_code_fence():
    md = "text\n\n```\n[[idea]]\n```\n"
    out = preprocess(md, "Notes/other.md", IDX, log=lambda *_: None)
    assert "[idea](idea.html)" not in out
    assert "[[idea]]" in out


def test_wikilink_not_rewritten_inside_inline_code():
    out = preprocess("run `[[idea]]` now", "Notes/other.md", IDX, log=lambda *_: None)
    assert "[[idea]]" in out


def test_image_embed_normalized_to_md_image():
    out = preprocess("![[_logo.png]]", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "![_logo.png](_logo.png)"


def test_note_transclusion_renders_placeholder():
    out = preprocess("![[idea]]", "Notes/other.md", IDX, log=lambda *_: None)
    assert "transclusion" in out and "idea" in out


def test_standard_md_link_extension_rewritten():
    out = preprocess("[a](idea.md)", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "[a](idea.html)"


def test_http_link_not_rewritten():
    out = preprocess("[a](https://example.com)", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "[a](https://example.com)"


def test_broken_link_is_logged():
    msgs = []
    preprocess("[[ghost]]", "Notes/other.md", IDX, log=lambda *a: msgs.append(a))
    assert msgs and "ghost" in msgs[0][0]
