"""Render pages/benchmarks/index.html with Playwright for visual verification.

Usage: python tools/render_benchmarks.py <out_prefix>
Produces <out_prefix>_chart.png (the core chart, 1280px wide) and
<out_prefix>_mobile.png (the core chart at 390px).
The page fetches live benchmark data from raw.githubusercontent.com (main).
"""
import sys, pathlib, asyncio
from playwright.async_api import async_playwright

PAGE = pathlib.Path(__file__).resolve().parents[1] / "pages" / "benchmarks" / "index.html"


async def shot(pw, url, path, width):
    browser = await pw.chromium.launch()
    page = await (await browser.new_context(viewport={"width": width, "height": 1400})).new_page()
    await page.goto(url, wait_until="networkidle", timeout=60000)
    await page.wait_for_timeout(2500)  # let Chart.js + annotation plugin paint
    el = await page.query_selector("#chart-core")
    await el.scroll_into_view_if_needed()
    await page.wait_for_timeout(500)
    await el.screenshot(path=path)
    await browser.close()


async def main(prefix):
    url = PAGE.as_uri()
    async with async_playwright() as pw:
        await shot(pw, url, f"{prefix}_chart.png", 1280)
        await shot(pw, url, f"{prefix}_mobile.png", 390)
    print(f"wrote {prefix}_chart.png and {prefix}_mobile.png")


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1] if len(sys.argv) > 1 else "bench"))
