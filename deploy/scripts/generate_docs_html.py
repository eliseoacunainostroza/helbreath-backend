#!/usr/bin/env python3
"""Genera documentación HTML estática desde fuentes Markdown del repositorio."""

from __future__ import annotations

import argparse
import html
import re
from pathlib import Path
from typing import Iterable


def parse_args() -> argparse.Namespace:
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description="Generar documentación HTML del proyecto")
    parser.add_argument(
        "--output",
        default=str(root / "docs" / "html"),
        help="Directorio de salida para los archivos HTML generados",
    )
    return parser.parse_args()


def discover_sources(root: Path) -> list[Path]:
    sources: list[Path] = []
    readme = root / "README.md"
    if readme.exists():
        sources.append(readme)

    docs_dir = root / "docs"
    if docs_dir.exists():
        for doc in sorted(docs_dir.glob("*.md")):
            if doc.name.lower() != "readme.md":
                sources.append(doc)
    return sources


def extract_title(markdown_text: str, fallback: str) -> str:
    for line in markdown_text.splitlines():
        match = re.match(r"^#\s+(.+)$", line.strip())
        if match:
            return match.group(1).strip()
    return fallback


def render_inline(text: str) -> str:
    parts = re.split(r"(`[^`]*`)", text)
    out: list[str] = []
    for part in parts:
        if not part:
            continue
        if part.startswith("`") and part.endswith("`") and len(part) >= 2:
            out.append(f"<code>{html.escape(part[1:-1])}</code>")
            continue
        escaped = html.escape(part)
        escaped = re.sub(
            r"\[([^\]]+)\]\(([^)]+)\)",
            lambda m: (
                f'<a href="{html.escape(m.group(2), quote=True)}">'
                f"{m.group(1)}"
                "</a>"
            ),
            escaped,
        )
        escaped = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", escaped)
        escaped = re.sub(r"\*(.+?)\*", r"<em>\1</em>", escaped)
        out.append(escaped)
    return "".join(out)


def parse_table_cells(line: str) -> list[str] | None:
    stripped = line.strip()
    if not (stripped.startswith("|") and stripped.endswith("|")):
        return None
    inner = stripped[1:-1]
    cells = [cell.strip().replace("\\|", "|") for cell in inner.split("|")]
    if len(cells) < 2:
        return None
    return cells


def is_table_separator(cells: list[str]) -> bool:
    if not cells:
        return False
    return all(re.match(r"^:?-{3,}:?$", cell.replace(" ", "")) for cell in cells)


def markdown_to_html(markdown_text: str) -> str:
    lines = markdown_text.splitlines()
    html_lines: list[str] = []

    in_code = False
    code_lang = ""
    code_buffer: list[str] = []
    list_type: str | None = None
    paragraph: list[str] = []
    table_rows: list[list[str]] = []

    def flush_paragraph() -> None:
        nonlocal paragraph
        if paragraph:
            text = " ".join(x.strip() for x in paragraph if x.strip())
            html_lines.append(f"<p>{render_inline(text)}</p>")
            paragraph = []

    def flush_list() -> None:
        nonlocal list_type
        if list_type is not None:
            html_lines.append(f"</{list_type}>")
            list_type = None

    def flush_table() -> None:
        nonlocal table_rows
        if not table_rows:
            return

        if len(table_rows) >= 2 and is_table_separator(table_rows[1]):
            header = table_rows[0]
            body = table_rows[2:]
        else:
            header = table_rows[0]
            body = table_rows[1:]

        width = len(header)
        html_lines.append("<table>")
        html_lines.append(
            "<thead><tr>"
            + "".join(f"<th>{render_inline(cell)}</th>" for cell in header)
            + "</tr></thead>"
        )
        if body:
            html_lines.append("<tbody>")
            for row in body:
                normalized = row[:width] + ([""] * max(0, width - len(row)))
                html_lines.append(
                    "<tr>"
                    + "".join(f"<td>{render_inline(cell)}</td>" for cell in normalized)
                    + "</tr>"
                )
            html_lines.append("</tbody>")
        html_lines.append("</table>")
        table_rows = []

    for raw in lines:
        line = raw.rstrip("\n")

        if in_code:
            if line.strip().startswith("```"):
                code_text = html.escape("\n".join(code_buffer))
                if code_lang == "mermaid":
                    html_lines.append(f'<pre class="mermaid">{code_text}</pre>')
                else:
                    class_attr = (
                        f' class="language-{html.escape(code_lang, quote=True)}"'
                        if code_lang
                        else ""
                    )
                    html_lines.append(f"<pre><code{class_attr}>{code_text}</code></pre>")
                in_code = False
                code_lang = ""
                code_buffer = []
            else:
                code_buffer.append(line)
            continue

        stripped = line.strip()

        if stripped.startswith("```"):
            flush_paragraph()
            flush_list()
            flush_table()
            in_code = True
            code_lang = stripped[3:].strip().lower()
            code_buffer = []
            continue

        if not stripped:
            flush_paragraph()
            flush_list()
            flush_table()
            continue

        table_cells = parse_table_cells(stripped)
        if table_cells is not None:
            flush_paragraph()
            flush_list()
            table_rows.append(table_cells)
            continue
        flush_table()

        heading = re.match(r"^(#{1,6})\s+(.+)$", stripped)
        if heading:
            flush_paragraph()
            flush_list()
            level = len(heading.group(1))
            html_lines.append(f"<h{level}>{render_inline(heading.group(2).strip())}</h{level}>")
            continue

        unordered = re.match(r"^[-*]\s+(.+)$", stripped)
        if unordered:
            flush_paragraph()
            if list_type != "ul":
                flush_list()
                list_type = "ul"
                html_lines.append("<ul>")
            html_lines.append(f"<li>{render_inline(unordered.group(1).strip())}</li>")
            continue

        ordered = re.match(r"^\d+\.\s+(.+)$", stripped)
        if ordered:
            flush_paragraph()
            if list_type != "ol":
                flush_list()
                list_type = "ol"
                html_lines.append("<ol>")
            html_lines.append(f"<li>{render_inline(ordered.group(1).strip())}</li>")
            continue

        flush_list()
        paragraph.append(stripped)

    flush_paragraph()
    flush_list()
    flush_table()

    return "\n".join(html_lines)


def page_template(title: str, nav_html: str, body_html: str) -> str:
    return f"""<!doctype html>
<html lang="es">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{html.escape(title)}</title>
    <link rel="stylesheet" href="assets/style.css" />
    <script type="module">
      import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';
      mermaid.initialize({{ startOnLoad: true, securityLevel: 'loose' }});
    </script>
  </head>
  <body>
    <header>
      <h1>Documentación Helbreath Backend</h1>
      <p>HTML estático generado desde Markdown.</p>
    </header>
    <div class="layout">
      <nav>
        {nav_html}
      </nav>
      <main>
        {body_html}
      </main>
    </div>
  </body>
</html>
"""


def stylesheet() -> str:
    return """* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: "Segoe UI", Arial, sans-serif;
  background: #f6f7fb;
  color: #1f2430;
}
header {
  padding: 16px 20px;
  background: #0f172a;
  color: #f8fafc;
}
header h1 { margin: 0 0 6px 0; font-size: 22px; }
header p { margin: 0; color: #cbd5e1; font-size: 14px; }
.layout {
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  min-height: calc(100vh - 84px);
}
nav {
  border-right: 1px solid #e5e7eb;
  background: #ffffff;
  padding: 16px;
}
nav ul { margin: 0; padding: 0; list-style: none; }
nav li { margin: 6px 0; }
nav a { text-decoration: none; color: #0f172a; }
nav a:hover { text-decoration: underline; }
main {
  padding: 24px 32px;
  max-width: 1100px;
}
h1, h2, h3, h4, h5, h6 { color: #0b1220; }
p { line-height: 1.55; }
code {
  background: #eef2ff;
  border-radius: 4px;
  padding: 0.1rem 0.35rem;
  font-family: "Consolas", "Courier New", monospace;
}
pre {
  overflow-x: auto;
  padding: 12px;
  border-radius: 8px;
  background: #111827;
  color: #e5e7eb;
}
pre code {
  background: transparent;
  padding: 0;
}
.mermaid {
  background: #ffffff;
  border: 1px solid #e5e7eb;
  color: #1f2430;
}
table { border-collapse: collapse; width: 100%; }
th, td { border: 1px solid #d1d5db; padding: 8px; text-align: left; }
@media (max-width: 900px) {
  .layout { grid-template-columns: 1fr; }
  nav { border-right: none; border-bottom: 1px solid #e5e7eb; }
}
"""


def safe_name(path: Path) -> str:
    if path.name.lower() == "readme.md":
        return "readme.html"
    return f"{path.stem}.html"


def build_nav(items: Iterable[tuple[str, str]]) -> str:
    links = "\n".join(f'<li><a href="{href}">{html.escape(title)}</a></li>' for href, title in items)
    return f"<ul>{links}</ul>"


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parents[2]
    out_dir = Path(args.output).resolve()
    assets_dir = out_dir / "assets"
    out_dir.mkdir(parents=True, exist_ok=True)
    assets_dir.mkdir(parents=True, exist_ok=True)

    sources = discover_sources(root)
    if not sources:
        raise SystemExit("No se encontraron documentos Markdown.")

    pages: list[tuple[str, str, str]] = []
    rendered: dict[str, tuple[str, str]] = {}

    for source in sources:
        raw = source.read_text(encoding="utf-8")
        title = extract_title(raw, source.stem)
        filename = safe_name(source)
        body = markdown_to_html(raw)
        rendered[filename] = (title, body)
        try:
            source_label = source.relative_to(root).as_posix()
        except ValueError:
            source_label = source.as_posix()
        pages.append((filename, title, source_label))

    index_sections = [
        "<h2>Documentos Disponibles</h2>",
        "<ul>",
        *[
            f'<li><a href="{filename}">{html.escape(title)}</a> '
            f"<code>{html.escape(src)}</code></li>"
            for filename, title, src in pages
        ],
        "</ul>",
        "<p>Generado por <code>deploy/scripts/generate_docs_html.py</code>.</p>",
    ]

    rendered["index.html"] = ("Índice de Documentación", "\n".join(index_sections))

    nav_items = [("index.html", "Índice de Documentación"), *[(p[0], p[1]) for p in pages]]
    nav = build_nav(nav_items)

    for filename, (title, body) in rendered.items():
        page_html = page_template(title, nav, body)
        (out_dir / filename).write_text(page_html, encoding="utf-8")

    (assets_dir / "style.css").write_text(stylesheet(), encoding="utf-8")
    print(f"[docs-html] se generaron {len(rendered)} archivos HTML en {out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
