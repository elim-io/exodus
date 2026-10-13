"""Build the site: every post in posts/ becomes dist/blog/<slug>.html, plus an index."""
import pathlib
import shutil

import markdown

ROOT = pathlib.Path(__file__).parent
DIST = ROOT / "dist"


def page(title, body):
    return f"<!doctype html><title>{title}</title><link rel=stylesheet href=/start/site.css>{body}"


def main():
    shutil.rmtree(DIST, ignore_errors=True)
    shutil.copytree(ROOT / "start", DIST / "start")
    (DIST / "blog").mkdir(parents=True)
    links = []
    for post in sorted((ROOT / "posts").glob("*.md")):
        html = markdown.markdown(post.read_text())
        with open(DIST / "blog" / f"{post.stem}.html", "w") as out:
            out.write(page(post.stem, html))
        links.append(f'<li><a href="/blog/{post.stem}">{post.stem}</a></li>')
    (DIST / "index.html").write_text(page("Tiles", (ROOT / "start" / "index.html").read_text()))
    (DIST / "blog" / "index.html").write_text(page("Blog", "<ul>" + "".join(links) + "</ul>"))


if __name__ == "__main__":
    main()
