from md2kindle.resolver import build_index, resolve


def _idx():
    return build_index(["Notes/Projects/design.md", "Notes/Inbox/idea.md", "Daily/today.md"])


def test_simple_basename_resolves_to_relative_html():
    idx = _idx()
    r = resolve("idea", idx, "Notes/Projects/design.md")
    assert r.broken is False
    assert r.href == "../Inbox/idea.html"
    assert r.text == "idea"


def test_alias_uses_alias_as_text():
    idx = _idx()
    r = resolve("idea|my cool note", idx, "Notes/Projects/design.md")
    assert r.href == "../Inbox/idea.html"
    assert r.text == "my cool note"


def test_partial_path_match():
    idx = _idx()
    r = resolve("Projects/design", idx, "Daily/today.md")
    assert r.href == "../Notes/Projects/design.html"


def test_heading_anchor_appended():
    idx = _idx()
    r = resolve("idea#Section One", idx, "Notes/Projects/design.md")
    assert r.href == "../Inbox/idea.html#section-one"


def test_same_file_anchor():
    idx = _idx()
    r = resolve("#Section One", idx, "Notes/Projects/design.md")
    assert r.href == "#section-one"
    assert r.broken is False


def test_duplicate_basenames_pick_shortest_path():
    idx = build_index(["a/x.md", "a/b/x.md"])
    r = resolve("x", idx, "Daily/today.md")
    assert r.href == "../a/x.html"


def test_broken_link_when_no_match():
    idx = _idx()
    r = resolve("nope", idx, "Notes/Projects/design.md")
    assert r.broken is True
    assert r.href is None
    assert r.text == "nope"


def test_top_level_source_resolves():
    # a wikilink resolved FROM a top-level note (no directory component) must
    # produce a correct relative href: the empty-`start` relpath case.
    idx = _idx()
    r = resolve("idea", idx, "root.md")
    assert r.broken is False
    assert r.href == "Notes/Inbox/idea.html"
    assert r.text == "idea"
    # top-level target from a top-level source resolves to a bare filename
    idx2 = build_index(["root.md", "idea.md"])
    r2 = resolve("idea", idx2, "root.md")
    assert r2.broken is False
    assert r2.href == "idea.html"
