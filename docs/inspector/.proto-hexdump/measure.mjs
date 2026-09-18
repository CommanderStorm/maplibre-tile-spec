// Real-clock measurements + interaction checks against the running prototype.
import puppeteer from "puppeteer-core";

const BASE = "http://localhost:5175";
const REAL = "real/omt_z14_8298_10748";

const cases = [
  { variant: "A", fixture: REAL, extra: "" },
  { variant: "A", fixture: REAL, extra: "&chunk=99999" },
  { variant: "A", fixture: REAL, extra: "&chunk=99999&maxBlob=0" },
  { variant: "B", fixture: REAL, extra: "" },
  { variant: "C", fixture: REAL, extra: "" },
  { variant: "C", fixture: REAL, extra: "&maxBlob=0" },
];

const browser = await puppeteer.launch({
  executablePath: "/usr/bin/google-chrome",
  args: ["--no-sandbox"],
  headless: true,
  defaultViewport: { width: 1700, height: 1000 },
});

for (const c of cases) {
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));

  const url = `${BASE}/?variant=${c.variant}&fixture=${c.fixture}${c.extra}`;
  const t0 = Date.now();
  await page.goto(url, { waitUntil: "networkidle0" });
  await page.waitForFunction(() => !document.querySelector(".status")?.textContent?.includes("timing"), {
    timeout: 120_000,
  });
  const load = Date.now() - t0;

  const status = await page.$eval(".status", (el) => el.textContent.trim());
  const nodes = await page.evaluate(() => document.querySelectorAll("*").length);

  // Time a view-only knob change on the real clock: type into max blob, wait for paint.
  const knob = await page.evaluate(async () => {
    const input = document.querySelector('input[type="range"]');
    const t = performance.now();
    input.value = String(Number(input.value) === 16 ? 24 : 16);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    return performance.now() - t;
  });

  console.log(`${c.variant}${c.extra || " (defaults)"}`);
  console.log(`  load-to-ready ${load} ms · width knob ${knob.toFixed(0)} ms · ${nodes} DOM nodes`);
  console.log(`  status: ${status}`);
  if (errors.length) console.log(`  ERRORS: ${errors.slice(0, 3).join(" | ")}`);
  await page.close();
}

// Interaction checks on the small v2 fixture: hover link, collapse, layer filter.
const page = await browser.newPage();
await page.goto(`${BASE}/?variant=C&fixture=0x02/mvalues_f64_alp`, { waitUntil: "networkidle0" });
await page.hover(".map .hexrow:nth-child(2) .cell:nth-child(6)");
const hovered = await page.$eval("aside h2", (el) => el.textContent.trim().split("\n")[0]);
const litCells = await page.$$eval(".cell.on", (els) => els.length);
console.log(`C hover: byte -> region "${hovered}", ${litCells} bytes lit`);

// A container lights its whole span, and ↑/↓ walk the leaves.
await page.evaluate(() => {
  [...document.querySelectorAll(".node.container")].find((n) => n.textContent.includes("geometry"))?.click();
});
const span = await page.$$eval(".cell.on", (els) => els.length);
await page.keyboard.press("ArrowDown");
const afterKey = await page.$eval("aside h2", (el) => el.textContent.trim().split("\n")[0]);
console.log(`C container span: ${span} bytes lit · ArrowDown -> "${afterKey}"`);

await page.goto(`${BASE}/?variant=B&fixture=0x02/nested_map_str`, { waitUntil: "networkidle0" });
const before = await page.$$eval(".node", (e) => e.length);
await page.click(".folds button:nth-child(1)");
const after = await page.$$eval(".node", (e) => e.length);
console.log(`B expand all: ${before} -> ${after} nodes`);

await page.goto(`${BASE}/?variant=A&fixture=${REAL}&layer=3`, { waitUntil: "networkidle0" });
const filtered = await page.$eval(".status", (el) => el.textContent.trim());
console.log(`A layer=3 filter: ${filtered}`);

await browser.close();
