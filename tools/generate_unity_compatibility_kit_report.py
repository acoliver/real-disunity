#!/usr/bin/env python3
"""Build the offline report from three documentation inputs and local citation paths.

Run: python3 tools/generate_unity_compatibility_kit_report.py
Requires existing markdown_it, /usr/bin/google-chrome and Poppler tools.
Uses embedded presentation resources and leaves runtime/game files unchanged.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import html
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET

from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
STEM = "unity-compatibility-kit-architecture"
INPUTS = (DOCS / f"{STEM}.md", DOCS / "unity-compatibility-kit-cases.md",
          DOCS / "unity-compatibility-kit-fixture.example.json")
MD = MarkdownIt("commonmark", {"html": False}).enable("table")
E = html.escape

CSS = r"""
@page { size: A4; margin: 17mm 17mm 19mm;
 @bottom-left { content: 'REAL DISUNITY / ENGINE INTEGRATION PLAN'; font: 7pt Arial, sans-serif; color: #526a7a; }
 @bottom-right { content: counter(page) ' / ' counter(pages); font: 8pt Arial, sans-serif; color: #526a7a; }
}
@page:first { @bottom-left { content: none; } }
* { box-sizing: border-box; }
html { color: #173047; background: #e8eef2; font-family: Arial, sans-serif; }
body { margin: 0 auto; max-width: 1020px; background: white; font-size: 10.5pt; line-height: 1.47; }
section { padding: 38px 50px; }
section > .kicker { break-after: avoid; }
h1,h2,h3,h4 { color: #11283d; line-height: 1.17; break-after: avoid; }
h1 { font-size: 32pt; letter-spacing: -1.1px; }
h2 { font-size: 22pt; margin: 0 0 22px; letter-spacing: -.5px; }
h3 { font-size: 14pt; margin-top: 25px; }
h4 { font-size: 11pt; margin: 0 0 9px; }
p { margin: 9px 0; }
a { color: #126777; text-decoration-thickness: 1px; }
strong { color: #102d43; }
code { font-family: 'DejaVu Sans Mono', monospace; font-size: .88em; overflow-wrap: anywhere; }
pre { font: 7.7pt/1.43 'DejaVu Sans Mono', monospace; white-space: pre-wrap; overflow-wrap: anywhere; background: #f0f5f7; border-left: 3px solid #3e8996; padding: 12px; break-inside: auto; }
pre code { font: inherit; }
li { margin: 5px 0; }
ul,ol { padding-left: 22px; }
.kicker { font-size: 9pt; font-weight: 700; letter-spacing: 2px; text-transform: uppercase; color: #137b87; }
.lead { font-size: 16pt; line-height: 1.4; }
.small { font-size: 9pt; color: #526a7a; }
.cover { min-height: 950px; position: relative; background: #102b40; color: #e2edf3; padding: 65px 55px; }
.cover h1 { color: white; font-size: 49pt; margin: 44px 0 20px; }
.cover .kicker { color: #7ad4d7; }
.cover .lead { max-width: 620px; }
.cover strong { color: white; }
.cover .rule { width: 92px; height: 7px; background: #e9ae58; margin: 28px 0; }
.cover footer { margin-top: 55px; font-size: 10pt; color: #c7dbe7; }
.cover-art { display: flex; align-items: center; gap: 15px; margin: 48px 0 32px; }
.cover-art span { border: 1px solid #527387; padding: 16px 18px; border-radius: 10px; font-size: 12pt; }
.cover-art b { color: #7ad4d7; font-size: 22pt; }
.grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
.tile { border: 1px solid #cedce3; border-top: 4px solid #187b88; border-radius: 5px; padding: 15px; break-inside: avoid; }
.tile h3 { margin: 0 0 8px; }
.toc { display: grid; grid-template-columns: 1fr 1fr; gap: 6px 25px; margin: 18px 0; }
.toc a { display: block; padding: 8px 0; border-bottom: 1px solid #dce6eb; text-decoration: none; }
.chapter { border-top: 6px solid #183e55; }
.figure { width: 100%; height: auto; display: block; margin: 20px 0; }
.caption { font-size: 9pt; color: #506b7b; }
.dashboard { background: #f1f5f7; }
.dash-head { background: #153348; color: white; padding: 20px; border-radius: 7px 7px 0 0; }
.dash-head h3 { color: white; margin: 0 0 7px; }
.badge { display: inline-block; border: 1px solid #a97420; color: #785115; background: #fff1d7; border-radius: 20px; padding: 4px 10px; font-size: 8pt; font-weight: bold; }
.filter { display: inline-block; margin: 8px 5px 0 0; padding: 6px 10px; background: #e4edf2; color: #2e4e61; border-radius: 4px; font-size: 8.5pt; }
.metrics { display: grid; grid-template-columns: repeat(3,1fr); gap: 10px; margin: 15px 0; }
.metric { background: white; padding: 13px; border: 1px solid #d0dfe6; border-radius: 5px; }
.metric b { display: block; font-size: 23pt; color: #173b50; }
.metric span { font-size: 8pt; }
.matrix { border: 1px solid #d1dfe6; background: white; border-radius: 4px; margin: 15px 0; }
.matrix-row { display: grid; grid-template-columns: 1.3fr repeat(3,1fr); border-bottom: 1px solid #e2e9ed; }
.matrix-row:last-child { border: none; }
.matrix-row span { padding: 9px; font-size: 8pt; }
.matrix-row.head { background: #dfeaf0; font-weight: bold; }
.dashboard .tile { background: white; font-size: 9pt; }
.dashboard .tile h3 { font-size: 11pt; }
.hash { font: 8.5pt/1.5 'DejaVu Sans Mono', monospace; overflow-wrap: anywhere; }
table { border-collapse: collapse; width: 100%; margin: 14px 0; font-size: 9pt; line-height: 1.35; table-layout: fixed; }
th, td { padding: 7px 9px; text-align: left; vertical-align: top; border-bottom: 1px solid #d4dfe6; overflow-wrap: anywhere; }
th { background: #e7eff3; color: #173b50; font-size: 8.5pt; }
thead { display: table-header-group; }
tr { break-inside: avoid; }
.case-card { border-top: 2px solid #d5963a; padding: 10px 0 8px; margin: 18px 0; break-inside: avoid; }
.case-card h4 { margin: 0 0 7px; }
.case-card h4 p { margin: 0; }
.case-field { margin: 6px 0; }
.case-field p { margin: 0; }
.case-label { font-size: 8pt; color: #4d6473; font-weight: bold; }
.case-label p { display: inline; }
.citations { font-size: 9pt; columns: 2; column-gap: 24px; }
.citations li { break-inside: avoid; }
#fixture-journey p { margin: 7px 0; }
.task-table table, .prerequisite-table table { margin: 10px 0; }
.task-table td, .task-table th, .prerequisite-table td, .prerequisite-table th { padding: 4px 7px; }
.task-table th:nth-child(1) { width: 14%; }
.task-table th:nth-child(2) { width: 29%; }
.task-table th:nth-child(3) { width: 44%; }
.task-table th:nth-child(4) { width: 13%; }
.prerequisite-table th:nth-child(1) { width: 48%; }
.prerequisite-table th:nth-child(2) { width: 15%; }
.prerequisite-table th:nth-child(3) { width: 37%; }
.hash-inventory td:last-child { font: 7.3pt/1.5 'DejaVu Sans Mono', monospace; }
.hash-inventory th:first-child { width: 28%; }
.hash-inventory th:nth-child(2) { width: 8%; }
.hash-inventory th:last-child { width: 64%; }
@media print {
 html,body { background: white; max-width: none; }
 body { font-size: 10.2pt; }
 section { padding: 0; margin-top: 7mm; break-before: auto; }
 .cover, #navigation, .visual, #dashboard-preview, #case-navigation, #catalog-source, #fixture-summary, #fixture-inputs, #fixture-journey, #source-citations, #regeneration { break-before: page; margin-top: 0; }
 section:first-child { break-before: auto; }
 .cover { min-height: 250mm; padding: 16mm 12mm; print-color-adjust: exact; }
 .cover h1 { font-size: 43pt; }
 .cover-art { margin-top: 20mm; }
 .chapter { padding-top: 5mm; border-top-width: 2px; }
 h2 { font-size: 20pt; margin-bottom: 14px; }
 .case-card { font-size: 9.5pt; line-height: 1.38; margin: 12px 0; padding: 8px 0 5px; }
 .case-field { margin: 5px 0; }
 .case-card h4 { font-size: 11pt; }
 .metric b { font-size: 20pt; }
 .dashboard { background: white; }
 .figure { max-height: 204mm; object-fit: contain; }
 .toc { font-size: 10pt; }
 #navigation .toc { font-size: 9pt; gap: 2px 22px; line-height: 1.25; margin: 12px 0; }
 #navigation .toc a { padding: 5px 0; }
 .dashboard { font-size: 9pt; line-height: 1.3; }
 .dashboard h3 { margin-top: 16px; }
 .dashboard .tile { font-size: 8.7pt; padding: 10px; }
 .dashboard .matrix-row span { padding: 8px; }
 .dashboard .small { font-size: 8.5pt; }
 .dashboard h2 { font-size: 21pt; margin-bottom: 16px; }
 .dashboard .dash-head { padding: 14px; }
 .dashboard .metric { padding: 10px; }
 .dashboard .metrics { margin: 12px 0; }
 .dashboard .grid { gap: 10px; }
 * { print-color-adjust: exact; -webkit-print-color-adjust: exact; }
}
"""


def svg_image(svg: str, alt: str) -> str:
    ET.fromstring(svg)
    encoded = base64.b64encode(svg.encode()).decode()
    return f'<img class="figure" alt="{E(alt)}" src="data:image/svg+xml;base64,{encoded}">'


def diagram(kind: str) -> str:
    parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="760" height="790" viewBox="0 0 760 790">',
             '<defs><marker id="a" markerWidth="9" markerHeight="9" refX="7" refY="4" orient="auto-start-reverse"><path d="M0 0L8 4L0 8" fill="none" stroke="#247886" stroke-width="1.4"/></marker></defs>',
             '<rect width="760" height="790" rx="12" fill="#f0f5f7"/>']

    def text(x: int, y: int, lines: list[str], size: int = 16, color: str = '#193c50') -> None:
        parts.append(f'<text x="{x}" y="{y}" text-anchor="middle" font-family="Arial,sans-serif" font-size="{size}" fill="{color}">')
        for i, line in enumerate(lines):
            parts.append(f'<tspan x="{x}" dy="{0 if i == 0 else 23}">{E(line)}</tspan>')
        parts.append('</text>')

    def box(x: int, y: int, w: int, h: int, lines: list[str], dark: bool = False) -> None:
        fill = '#153c53' if dark else '#fff'
        parts.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" '
                     f'rx="9" fill="{fill}" stroke="#9cbac7"/>')
        text(x + w // 2, y + 30, lines, 16, '#fff' if dark else '#193c50')

    def arrow(x1: int, y1: int, x2: int, y2: int, both: bool = False) -> None:
        start_marker = ' marker-start="url(#a)"' if both else ''
        parts.append(f'<path d="M{x1} {y1}L{x2} {y2}" stroke="#247886" '
                     f'stroke-width="2" fill="none" marker-end="url(#a)"{start_marker}/>')

    if kind == 'architecture':
        box(165, 22, 430, 68, ['Offline supervisor', 'Fixtures • profiles • budgets • evidence'])
        arrow(380, 90, 380, 120)
        box(165, 120, 430, 68, ['HostSession / RunControl', 'OS + replay ingress • one native schedule'])
        arrow(380, 188, 380, 225)
        box(155, 225, 450, 115, ['Canonical Rust EngineSession', 'Entities • components • transform graph', 'Clock • input • physics • resources'], True)
        box(23, 383, 325, 68, ['Native command / query ABI', 'Validated handles • bounded buffers'])
        arrow(230, 340, 185, 383, True)
        box(412, 383, 325, 68, ['Managed phase / event ABI', 'Dispatch without world locks held'])
        arrow(570, 340, 575, 383, True)
        box(155, 497, 450, 68, ['C# bridge + UnityEngine-shaped facades', 'Public views of canonical engine components'])
        arrow(185, 451, 270, 497, True)
        arrow(575, 451, 490, 497, True)
        box(155, 605, 450, 68, ['Unchanged GameScripts.dll + public cases', 'Read-only source • explicit content overlays'])
        arrow(380, 565, 380, 605, True)
        text(380, 722, ['Canonical camera / resolved view → RenderScene', 'Shared pipelines → surface / offscreen → correlated capture'], 16)
        parts.append('<path d="M605 280L738 280L738 695L650 695" fill="none" stroke="#247886" stroke-width="2" marker-end="url(#a)"/>')
        text(380, 779, ['Planned composition; current implementation is listed in chapter 1'], 12)
    else:
        steps = [
            ('01', 'Prepare + load', 'Validate locks, profiles and prerequisites before activation.'),
            ('02', 'Replay / OS event', 'Timestamped inputs meet at the same normalized ingress.'),
            ('03', 'Fixed phase + physics', 'Advance the sole world; deliver real contacts without locks.'),
            ('04', 'Update + observations', 'Public state and callback evidence carry phase / tick stamps.'),
            ('05', 'Render + present + capture', 'One resolved camera; capture the tagged presented snapshot.'),
            ('06', 'Gameplay checkpoint', 'Correlate public state and pixels with the declared action.'),
            ('07', 'Reload + renewed control', 'Reload through the engine and verify renewed control.'),
            ('08', 'Stop + drain + index', 'Drain roots/worlds; finish readbacks and index artifacts.'),
        ]
        for i, (num, title, detail) in enumerate(steps):
            y = 22+i*91
            parts.append(f'<circle cx="49" cy="{y+29}" r="22" fill="#163e54"/>')
            text(49, y+35, [num], 15, '#fff')
            box(92, y, 643, 73, [title, detail])
            if i < 7:
                arrow(49, y+51, 49, y+91-22)
        text(380, 777, ['Planned timeline; simulation and wall deadlines are recorded separately'], 12)
    parts.append('</svg>')
    return svg_image(''.join(parts), 'Single-world architecture with bidirectional C# / Rust ABI' if kind == 'architecture' else 'Generic replay, capture, reset and teardown timeline')


def slug(text: str) -> str:
    return re.sub(r'[^a-z0-9]+', '-', text.lower()).strip('-')


def internalize_links(tokens: list, prefix: str, citations: dict[str, str]) -> None:
    targets = {INPUTS[0].name: '#architecture-source', INPUTS[1].name: '#catalog-source',
               INPUTS[2].name: '#fixture-summary', f'{STEM}.html': '#cover',
               f'{STEM}.pdf': '#cover'}
    for token in tokens:
        if token.children:
            internalize_links(token.children, prefix, citations)
        if token.type != 'link_open':
            continue
        href = token.attrGet('href') or ''
        path, _, fragment = href.partition('#')
        if not path:
            token.attrSet('href', f'#{prefix}-{slug(fragment)}')
        elif path in targets:
            target = targets[path]
            token.attrSet('href', target + ('-' + slug(fragment) if fragment else ''))
        else:
            local = (DOCS / path).resolve()
            local.relative_to(ROOT)
            if not local.is_file():
                raise ValueError(f'Missing local citation target: {href}')
            ident = 'citation-' + hashlib.sha256(href.encode()).hexdigest()[:12]
            citations[href] = ident
            token.attrSet('href', '#' + ident)


def source_citations(citations: dict[str, str]) -> str:
    entries = []
    for href, ident in citations.items():
        path, _, fragment = href.partition('#')
        target = (DOCS / path).resolve().as_uri()
        if fragment:
            target += '#' + fragment
        entries.append(f'<li id="{ident}"><a href="{E(target, quote=True)}"><code>{E(href)}</code></a></li>')
    return ('<section class="chapter" id="source-citations"><p class="kicker">Local primary sources</p>'
            '<h2>Repository source references</h2><p>Inline citations lead here in both editions. '
            'Each path links to the local checkout used for this build. The HTML and PDF contain '
            'all report prose; opening an implementation or reference file requires this checkout.</p>'
            '<ol class="citations">' + ''.join(entries) + '</ol></section>')


def render(tokens: list) -> str:
    return MD.renderer.render(tokens, MD.options, {})


def render_table(tokens: list) -> str:
    rows, cells, current = [], [], None
    for token in tokens:
        if token.type == 'tr_open':
            cells = []
        elif token.type in ('th_open', 'td_open'):
            current = []
        elif token.type in ('th_close', 'td_close'):
            cells.append(current)
            current = None
        elif token.type == 'tr_close':
            rows.append(cells)
        elif current is not None:
            current.append(token)
    headers = [render(cell) for cell in rows[0]]
    is_catalog = any(re.search(r'UCK-[A-Z]+-\d{3}', token.content)
                     for row in rows[1:] for token in row[0])
    if not is_catalog:
        return '<div class="table-wrap">' + render(tokens) + '</div>'
    result = [f'<div class="case-label">{headers[0]}</div>']
    for row in rows[1:]:
        contents = [render(cell) for cell in row]
        case_id = re.search(r'UCK-[A-Z]+-\d{3}', ' '.join(t.content for t in row[0]))[0]
        result.append(f'<article class="case-card" id="{case_id}"><h4>{contents[0]}</h4>')
        for label, content in zip(headers[1:], contents[1:]):
            result.append(f'<div class="case-field"><div class="case-label">{label}</div>{content}</div>')
        result.append('</article>')
    return ''.join(result)


def render_source(source: str, prefix: str, citations: dict[str, str]) -> tuple[str, list[tuple[str, str]]]:
    tokens = MD.parse(source)
    internalize_links(tokens, prefix, citations)
    out, toc, pending = [], [], []
    opened = False
    i = 0
    while i < len(tokens):
        token = tokens[i]
        if token.type == 'heading_open' and token.tag in ('h1', 'h2'):
            if pending:
                out.append(render(pending))
                pending = []
            if opened:
                out.append('</section>')
            title = tokens[i+1].content
            ident = prefix if token.tag == 'h1' else prefix + '-' + slug(title)
            out.append(f'<section class="chapter" id="{ident}"><p class="kicker">{"Architecture" if prefix == "architecture-source" else "Case catalog"}</p>')
            out.append(f'<h2>{E(title)}</h2>')
            opened = True
            if token.tag == 'h2':
                toc.append((ident, title))
            i += 3
        elif token.type == 'table_open':
            if pending:
                out.append(render(pending))
                pending = []
            end = i+1
            while tokens[end].type != 'table_close':
                end += 1
            out.append(render_table(tokens[i:end+1]))
            i = end+1
        else:
            pending.append(token)
            i += 1
    out.append(render(pending))
    if opened:
        out.append('</section>')
    return ''.join(out), toc


def dashboard() -> str:
    matrix = ''.join('<div class="matrix-row"><span>' + E(name) + '</span>'
                     + '<span>Unmeasured</span>' * 3 + '</div>'
                     for name in ['Objects / lifecycle', 'Input / clock', 'Camera / sprites', 'Physics / UI / assets'])
    cards = [
        ('Snake', '25 source requirements · modern', 'Original sprite missing. A substitute-content variant needs its own asset digest and label.'),
        ('Breakout', '43 source requirements · modern', 'Physics, tags, stale fields and selected URP or translated-unlit profile require implementation decisions.'),
        ('Historical Pong', '27 source requirements · 2017.1', 'Legacy prefab/events, score UI and body reset need integration. Eleven historical tests form a separate population.'),
        ('rd-stranded', 'Exploration lane · migration', 'Migrate host, input, camera and capture while preserving game-specific content and render capabilities.'),
    ]
    tiles = ''.join(f'<div class="tile"><h3>{title}</h3><div class="small">{subtitle}</div><p>{detail}</p></div>'
                    for title, subtitle, detail in cards)
    return f'''<section class="dashboard" id="dashboard-preview"><p class="kicker">Dashboard layout specification</p>
<h2>Profiles, prerequisites and case results</h2>
<div class="dash-head"><h3>REAL DISUNITY / COMPATIBILITY KIT</h3><span class="badge">DESIGN PREVIEW · EXECUTION PENDING</span><p>Run: unavailable &nbsp; Engine revision: unselected &nbsp; Gate: unselected</p></div>
<div><span class="filter">Source: 6000.3 / 2017.1</span><span class="filter">Package: separate selection</span><span class="filter">Layer / OS / backend</span></div>
<div class="metrics"><div class="metric"><b>54</b><span>Proposed case definitions</span></div><div class="metric"><b>50 + 4</b><span>Compatibility-facing + harness</span></div><div class="metric"><b>Pending</b><span>Pass/fail totals require execution</span></div></div>
<h3>Subsystem × execution profile</h3><div class="matrix"><div class="matrix-row head"><span>Subsystem</span><span>Modern / simulation</span><span>Modern / desktop</span><span>Legacy / desktop</span></div>{matrix}</div>
<div class="grid">{tiles}</div><p class="small">The separate traceability panel tracks 95 gameplay/support requirement instances. Each result links its expectation revision, prerequisite chain, public observations and artifacts. A playable label requires completion of the fixture’s declared journey.</p></section>'''


def fixture_summary(fixture: dict) -> str:
    def table(headers: list[str], rows: list[list[str]]) -> str:
        head = ''.join(f'<th>{E(value)}</th>' for value in headers)
        body = ''.join('<tr>' + ''.join(f'<td>{E(value)}</td>' for value in row) + '</tr>' for row in rows)
        return f'<table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table>'

    f = fixture
    identity = [
        ['Fixture', f['fixtureId']], ['Source pin', f['source']['pin']],
        ['Scene', f['entryScene']], ['Source API', f['profiles']['sourceApi']],
        ['ABI / bridge', f['profiles']['nativeAbi'] + ' / ' + f['profiles']['bridgeProtocol']],
        ['Import / rendering', f['profiles']['importer'] + ' / ' + f['profiles']['rendering']],
        ['Compilation', '.NET 8; independent GameScripts.dll; SDK/language/symbol/assembly locks pending.'],
        ['Content variant', 'Original content unresolved; source edits disabled; optional substitute variant disabled.'],
        ['Input lock', '; '.join(f['source']['completeInputLock']['mustInclude'])],
    ]
    parts = ['<section class="chapter" id="fixture-summary"><p class="kicker">Fixture specimen</p>'
             '<h2>Snake admission and replay inputs</h2><p>This non-executable design specimen records source '
             'hashes and a planned journey. Its complete lock, original sprite, selectors and implementations '
             'remain prerequisites. The presentation build reproduces the recorded data without inspecting the game tree.</p>',
             table(['Selection', 'Recorded value'], identity)]
    packages = [[item['id'], item['version'], item['use']] for item in f['profiles']['packageMetadata']]
    parts.extend(['<h3>Package metadata</h3>', table(['Package', 'Version', 'Selected use'], packages),
                  '<p>The manifest excludes Editor/test graphs from gameplay compilation, declares zero native plugins '
                  'and selects independently implemented facade references. Imported config is ' + E(json.dumps(f['expectedImportedConfig'])) + '.</p></section>'])
    parts.append('<section class="chapter" id="fixture-inputs"><p class="kicker">Recorded input inventory</p>'
                 '<h2>Script and selected content hashes</h2><p>The specimen contains eight script records and seven '
                 'selected content records. These records are part of the future complete-input lock.</p>')
    records = [[item['path'], str(item.get('bytes', '')), item['sha256']]
               for item in f['source']['scripts'] + f['source']['selectedContent']]
    parts.append('<div class="hash-inventory">' + table(['Path', 'Bytes', 'SHA-256'], records) + '</div></section>')
    parts.append('<section class="chapter" id="fixture-journey"><p class="kicker">Finite journey contract</p>'
                 '<h2>Tasks, evidence and teardown</h2><p>Seed 1701; dynamic 1/60 s; fixed 1/50 s; at most 3600 frames '
                 'or 60 simulated seconds. The W/A timeline exercises steering. Food routing uses a separate bounded '
                 'driver over public observations. Desktop execution selects an admitted Vulkan adapter at 1280×720, DPI 1.</p>')
    tasks = [[item['id'], item['driver'], item.get('completion', item.get('state', '')),
              str(item.get('deadlineFrames', item.get('maxFrames', item.get('maxWallSeconds', 'declared'))))
              + (' wall s' if 'maxWallSeconds' in item else ' frames')] for item in f['tasks']]
    parts.append('<div class="task-table">' + table(['Task', 'Driver', 'Completion / state', 'Bound'], tasks) + '</div>')
    parts.append('<h3>Admission prerequisites</h3>')
    parts.append('<div class="prerequisite-table">' + table(['ID', 'State', 'Scope'], [[item['id'], item['state'], item['scope']] for item in f['prerequisites']]) + '</div>')
    parts.append('<p>Missing sprite GUID <code>311925a002f4447b3a28927169b83ea6</code>, fileID '
                 '<code>7482667652216324306</code>, blocks original-content visuals. The material translation '
                 'also requires implementation and validation under its selected fidelity profile.</p>')
    parts.append('<p>Observe public CurrentFoodPos, active SpriteRenderer positions/enabled state, derived active '
                 'segment count and the Game Over log. Validate clone-parent selectors. Assert original bytes, '
                 'input-correlated state/pixels, later food/growth, reload and renewed movement, with a linked exploration record.</p>')
    parts.append('<p>Captures: ' + E('; '.join(item['id'] + ': ' + item['purpose'] for item in f['screenshots'])) +
                 '. Outputs: ' + E('; '.join(f['artifacts']['formats'])) + '.</p>')
    parts.append('<p>Budgets: 120 wall seconds; 1 GiB resident where enforceable; 10 MiB logs; 16 captures; '
                 '256 MiB per readback; 512 MiB artifacts; two compile jobs. Drain roots/worlds, complete or cancel '
                 'readbacks, recheck source bytes and close only owned windows/processes.</p></section>')
    return ''.join(parts)


def build_html(sources: tuple[str, str, str]) -> tuple[str, list[str]]:
    architecture, cases, specimen = sources
    expected = re.findall(r'^\| \*\*(UCK-[A-Z]+-\d{3})', cases, re.M)
    if len(expected) != 54 or len(set(expected)) != 54 or sum(x.startswith('UCK-HAR-') for x in expected) != 4:
        raise ValueError('Expected exactly 54 unique case definitions: 50 compatibility + 4 harness')
    citations: dict[str, str] = {}
    architecture_html, architecture_toc = render_source(architecture, 'architecture-source', citations)
    catalog_html, catalog_toc = render_source(cases, 'catalog-source', citations)
    toc = ''.join(f'<a href="#{ident}">{E(title)}</a>' for ident, title in architecture_toc)
    case_toc = ''.join(f'<a href="#{ident}">{E(title)}</a>' for ident, title in catalog_toc)
    fingerprints = ''.join(
        f'<p><strong>{E(path.name)}</strong><br>'
        f'<span class="hash">{hashlib.sha256(content.encode()).hexdigest()}</span></p>'
        for path, content in zip(INPUTS, sources)
    )
    cover = '''<section class="cover" id="cover"><p class="kicker">Real Disunity / engineering design</p>
<h1>Unity-shaped<br>compatibility kit</h1><div class="rule"></div>
<p class="lead">Engine integration and test plan</p>
<div class="cover-art"><span>Unchanged C# source<br>Public facade contracts</span><b>↔</b><span>Rust simulation<br>Shared host and renderer</span></div>
<p>Architecture, case catalog and Snake fixture specimen</p>
<footer>DESIGN REVISION 1 / 20 OCTOBER 2026<br><br>54 proposed case definitions: 50 compatibility-facing and 4 harness engineering.<br>Runtime implementation and game execution remain planned.</footer></section>'''
    front = f'''<section id="navigation"><p class="kicker">Report navigation</p><h2>Integration plan and contract reference</h2>
<p>The architecture follows the developer workflow from engine ownership and managed dispatch to execution, game admission and reporting. Appendices retain ABI layouts, profile dimensions and the fixture schema.</p>
<div class="toc">{toc}</div><h3>Diagrams and implementation backlog</h3>
<div class="toc"><a href="#architecture-visual">Engine and bridge boundaries</a><a href="#replay-visual">Replay and capture sequence</a>
<a href="#dashboard-preview">Dashboard layout</a><a href="#catalog-source">54-case catalog</a>
<a href="#fixture-summary">Snake specimen and recorded hashes</a><a href="#source-citations">Local source references</a>
<a href="#regeneration">Build and validation</a></div></section>
<section class="visual" id="architecture-visual"><p class="kicker">Planned engine composition</p><h2>Native state and managed dispatch</h2>{diagram('architecture')}
<p class="caption">Commands, queries and callbacks reach the same EngineSession. The native schedule releases locks before managed callbacks. Render snapshots and the resolved camera feed the shared surface/offscreen path.</p></section>
<section class="visual" id="replay-visual"><p class="kicker">Worker execution sequence</p><h2>Input, gameplay observations and capture</h2>{diagram('timeline')}
<p class="caption">Simulation time, frame/fixed-tick stamps and wall deadlines have separate roles. Synthetic replay and OS input delivery retain their own evidence labels.</p></section>'''
    catalog_nav = f'''<section id="case-navigation"><p class="kicker">Case reference</p><h2>Catalog groups and promotion</h2>
<p>The 54 definitions below retain stable case IDs, public stimuli, prerequisites and traceability. Pending decisions require an approved expectation before baseline promotion. The architecture defines status and gate policy.</p><div class="toc">{case_toc}</div></section>'''
    regeneration = f'''<section class="chapter" id="regeneration"><p class="kicker">Offline presentation build</p><h2>Regeneration and artifact checks</h2>
<p>From the repository root:</p><pre>python3 tools/generate_unity_compatibility_kit_report.py</pre>
<p>The build uses existing Python 3, markdown_it, /usr/bin/google-chrome, pdfinfo and pdftotext. It reads the three documentation inputs below and checks local citation targets. Embedded CSS/SVG and a restrictive content policy keep the report self-contained. Chrome prints through a fresh temporary profile with background networking, extensions and sync disabled and network hosts mapped to an unreachable address.</p>
<pre>pdfinfo docs/{STEM}.pdf
pdftotext -layout docs/{STEM}.pdf /tmp/uck-report.txt
pdftoppm -f 1 -singlefile -scale-to 1400 -png \\
  docs/{STEM}.pdf /tmp/uck-cover</pre>
<p>Validation checks 54 exact catalog identities, the 50/4 split, complete source prose/code/table text in HTML and PDF, section markers, internal navigation, local citation targets and PDF word bounds. Rasterized page inspection is a separate review step. These checks establish presentation integrity; runtime and game acceptance remain implementation work.</p>
<h3>Exact documentation input SHA-256</h3>{fingerprints}</section>'''
    csp = (
        "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src 'none'; "
        "script-src 'none'; connect-src 'none'; object-src 'none'; base-uri 'none'; "
        "form-action 'none'; frame-src 'none'"
    )
    body = ''.join((cover, front, dashboard(), architecture_html, catalog_nav,
                    catalog_html, fixture_summary(json.loads(specimen)),
                    source_citations(citations), regeneration))
    document = (
        '<!doctype html><html lang="en"><head><meta charset="utf-8">'
        f'<meta http-equiv="Content-Security-Policy" content="{E(csp, quote=True)}">'
        '<meta name="viewport" content="width=device-width,initial-scale=1">'
        '<title>Real Disunity | Compatibility kit | Engine integration plan</title>'
        f'<style>{CSS}</style></head><body>{body}</body></html>'
    )
    validate_html(document, sources, expected)
    return document, expected


class ReportParser(HTMLParser):
    def __init__(self, check_links: bool = True) -> None:
        super().__init__()
        self.check_links = check_links
        self.text: list[str] = []
        self.case_ids: list[str] = []
        self.ids: list[str] = []
        self.anchors: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        if tag in ('script', 'iframe', 'object', 'link'):
            raise ValueError(f'Unexpected external/executable element: {tag}')
        if 'id' in values:
            self.ids.append(values['id'])
        if tag == 'a' and self.check_links:
            href = values.get('href', '')
            if href.startswith('#'):
                self.anchors.append(href[1:])
            elif href.startswith(ROOT.as_uri() + '/'):
                path = Path(href.removeprefix('file://').split('#')[0])
                if not path.is_file():
                    raise ValueError(f'Missing local citation: {href}')
            else:
                raise ValueError(f'Unexpected report link: {href}')
        if tag == 'article' and 'case-card' in values.get('class', ''):
            self.case_ids.append(values['id'])
        if 'src' in values and not values['src'].startswith('data:image/svg+xml;base64,'):
            raise ValueError('Report resource is not an embedded SVG')

    def handle_data(self, data: str) -> None:
        self.text.append(data)


def normalized_words(text: str) -> list[str]:
    return re.findall(r'\w+', text.casefold())


def validate_html(document: str, sources: tuple[str, str, str], expected: list[str]) -> None:
    parser = ReportParser()
    parser.feed(document)
    if parser.case_ids != expected:
        raise ValueError('Catalog cards differ from the 54 source definitions')
    if '\u2014' in document:
        raise ValueError('Em dash in report presentation')
    if len(parser.ids) != len(set(parser.ids)):
        raise ValueError('Duplicate report navigation IDs')
    if set(parser.anchors) - set(parser.ids):
        raise ValueError(f'Broken internal navigation: {set(parser.anchors) - set(parser.ids)}')
    fixture = json.loads(sources[2])
    for record in fixture['source']['scripts'] + fixture['source']['selectedContent']:
        if record['sha256'] not in document:
            raise ValueError(f'Fixture hash omitted: {record["path"]}')
    text_words = normalized_words(' '.join(parser.text))
    # Every source prose/code/table cell must survive presentation conversion.
    for source in sources[:2]:
        for token in MD.parse(source):
            if token.type not in ('inline', 'fence', 'code_block'):
                continue
            leaf = ReportParser(check_links=False)
            leaf.feed(MD.renderInline(token.content) if token.type == 'inline' else E(token.content))
            words = normalized_words(' '.join(leaf.text))
            if not words:
                continue
            width = len(words)
            if not any(text_words[i:i+width] == words for i, w in enumerate(text_words) if w == words[0]):
                raise ValueError(f'Source text omitted: {token.content[:140]}')


def print_pdf(html_path: Path, pdf_path: Path, temp: Path) -> None:
    command = [
        '/usr/bin/google-chrome', '--headless=new', '--disable-gpu',
        '--disable-dev-shm-usage', '--disable-background-networking',
        '--disable-component-update', '--disable-domain-reliability',
        '--disable-sync', '--disable-extensions', '--no-first-run',
        '--no-default-browser-check', '--metrics-recording-only', '--no-pdf-header-footer',
        '--no-proxy-server', '--host-resolver-rules=MAP * 0.0.0.0',
        f'--user-data-dir={temp / "chrome-profile"}',
        '--allow-file-access-from-files', '--run-all-compositor-stages-before-draw',
        f'--print-to-pdf={pdf_path}', html_path.as_uri(),
    ]
    result = subprocess.run(command, capture_output=True, text=True, timeout=150)
    if result.returncode or not pdf_path.exists():
        raise RuntimeError(f'Chrome print failed ({result.returncode}): {result.stderr[-5000:]}')


def validate_pdf(pdf_path: Path, expected: list[str], temp: Path, sources: tuple[str, str, str]) -> dict:
    info = subprocess.run(['pdfinfo', str(pdf_path)], check=True, capture_output=True, text=True).stdout
    text_path = temp / 'report.txt'
    subprocess.run(['pdftotext', '-layout', str(pdf_path), str(text_path)], check=True)
    text = text_path.read_text()
    pages = re.search(r'^Pages:\s+(\d+)', info, re.M)
    if not pages or int(pages[1]) < 10:
        raise ValueError('Unexpected PDF pagination')
    found = set(re.findall(r'UCK-[A-Z]+-\d{3}', text))
    if found != set(expected):
        raise ValueError(f'PDF case ID mismatch: missing={set(expected)-found}, extra={found-set(expected)}')
    markers = [token.content for source in sources[:2] for token in MD.parse(source)
               if token.type == 'inline' and token.map and
               source.splitlines()[token.map[0]].startswith('## ')]
    markers += ['non-executable design specimen', 'Regeneration and artifact checks',
                'DESIGN PREVIEW', 'rd-stranded', '311925a002f4447b3a28927169b83ea6']
    normalized = ' '.join(text.split())
    for marker in markers:
        if marker.casefold() not in normalized.casefold():
            raise ValueError(f'Required PDF marker missing: {marker}')
    page_map = {}
    for name, marker in [('cover', 'Unity-shaped'), ('dashboard', 'DESIGN PREVIEW'),
                         ('architecture', 'Native state and managed dispatch'),
                         ('timeline', 'Input, gameplay observations and capture'),
                         ('bridge', 'Bidirectional phase contract'),
                         ('fixture', 'Script and selected content hashes'),
                         ('case-first', 'UCK-INV-001 · P0'), ('case-physics', 'UCK-PHY-002 · P1'),
                         ('case-last', 'UCK-HAR-004 · P0')]:
        matches = [i+1 for i, page in enumerate(text.split('\f')) if marker in page]
        if not matches:
            raise ValueError(f'Could not locate visual review page: {name}')
        page_map[name] = matches[-1] if name != 'cover' else matches[0]
    raw_path = temp / 'reading-order.txt'
    subprocess.run(['pdftotext', '-raw', str(pdf_path), str(raw_path)], check=True)
    reading_order = re.sub(r'(?:\d+ / \d+\s*)?REAL DISUNITY / ENGINE INTEGRATION PLAN(?:\s*\d+ / \d+)?', '', raw_path.read_text())
    def compact(value: str) -> str:
        return ''.join(char for char in value.casefold() if char.isalnum())
    compact_pdf = compact(reading_order)
    fragment_count = 0
    for source in sources[:2]:
        for token in MD.parse(source):
            if token.type not in ('inline', 'fence', 'code_block'):
                continue
            fragment = ReportParser(check_links=False)
            fragment.feed(MD.renderInline(token.content) if token.type == 'inline' else E(token.content))
            expected_text = compact(' '.join(fragment.text))
            if expected_text and expected_text not in compact_pdf:
                raise ValueError(f'PDF source text omitted: {token.content[:140]}')
            fragment_count += 1
    fixture = json.loads(sources[2])
    for record in fixture['source']['scripts'] + fixture['source']['selectedContent']:
        if compact(record['sha256']) not in compact_pdf:
            raise ValueError(f'PDF fixture hash omitted: {record["path"]}')
    bbox_path = temp / 'word-bounds.html'
    subprocess.run(['pdftotext', '-bbox', str(pdf_path), str(bbox_path)], check=True)
    ns = {'x': 'http://www.w3.org/1999/xhtml'}
    word_count = 0
    for number, page in enumerate(ET.parse(bbox_path).findall('.//x:page', ns), 1):
        width, height = float(page.attrib['width']), float(page.attrib['height'])
        for word in page.findall('x:word', ns):
            bounds = {key: float(value) for key, value in word.attrib.items()}
            if (bounds['xMin'] < 0 or bounds['yMin'] < 0 or
                    bounds['xMax'] > width or bounds['yMax'] > height):
                raise ValueError(f'Word outside page {number}: {word.text}')
            word_count += 1
    if 'file:///tmp/' in text:
        raise ValueError('Browser temporary file URL leaked into running text')
    return {'pages': int(pages[1]), 'unique_case_ids': len(found),
            'source_text_fragments_retained': fragment_count,
            'words_checked_inside_page_bounds': word_count,
            'compatibility_definitions': 50, 'harness_definitions': 4,
            'fixture_hashes_retained': len(fixture['source']['scripts'] + fixture['source']['selectedContent']),
            'required_section_markers': len(markers), 'visual_review_pages': page_map}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--html-only', action='store_true', help='Build and validate HTML without printing')
    args = parser.parse_args()
    sources = tuple(path.read_text() for path in INPUTS)
    document, expected = build_html(sources)
    html_path, pdf_path = DOCS / f'{STEM}.html', DOCS / f'{STEM}.pdf'
    # Validate before replacing delivered files. Both browser state and intermediate
    # output live in an isolated system temporary directory, not the repository.
    with tempfile.TemporaryDirectory(prefix='uck-presentation-') as tmp:
        temp = Path(tmp)
        temporary_html = temp / html_path.name
        temporary_html.write_text(document)
        if args.html_only:
            html_path.write_text(document)
            print(f'HTML validated: {html_path}; 54 exact catalog cards; complete source text retained')
            return
        temporary_pdf = temp / pdf_path.name
        print_pdf(temporary_html, temporary_pdf, temp)
        validation = validate_pdf(temporary_pdf, expected, temp, sources)
        html_path.write_text(document)
        pdf_path.write_bytes(temporary_pdf.read_bytes())
    print(json.dumps({'html': str(html_path), 'pdf': str(pdf_path), 'validation': validation}, indent=2))


if __name__ == '__main__':
    main()
