// Screenshots with a selection made, which a plain headless --screenshot cannot do.
import puppeteer from "puppeteer-core";

const BASE = "http://localhost:5175";
const out = process.argv[2] ?? ".";

const browser = await puppeteer.launch({
  executablePath: "/usr/bin/google-chrome",
  args: ["--no-sandbox"],
  headless: true,
  defaultViewport: { width: 1700, height: 1000 },
});

async function shot(name, url, prepare) {
  const page = await browser.newPage();
  await page.goto(url, { waitUntil: "networkidle0" });
  if (prepare) await prepare(page);
  await page.screenshot({ path: `${out}/${name}.png` });
  await page.close();
}

// C, real tile, a data blob selected: inspector pane with blob info and decoded values.
await shot("c_real_selected", `${BASE}/?variant=C&fixture=real/omt_z14_8298_10748`, async (page) => {
  await page.evaluate(() => {
    const at = [...document.querySelectorAll(".list li")].find((li) => li.className.includes("dataBlob"));
    at?.click();
  });
  await new Promise((r) => setTimeout(r, 200));
});

// C, a bit-packed byte selected: the bit breakdown against the real byte.
await shot("c_bits", `${BASE}/?variant=C&fixture=0x02/mvalues_f64_alp`, async (page) => {
  await page.evaluate(() => {
    const at = [...document.querySelectorAll(".list li")].find((li) => li.textContent.trim() === "layout");
    at?.click();
  });
  await new Promise((r) => setTimeout(r, 200));
});

// B, real tile, one layer expanded two levels down.
await shot("b_real", `${BASE}/?variant=B&fixture=real/omt_z14_8298_10748`, async (page) => {
  await page.evaluate(() => {
    document.querySelectorAll(".node .caret")[2]?.click();
  });
  await new Promise((r) => setTimeout(r, 200));
  await page.evaluate(() => {
    const blob = [...document.querySelectorAll(".node")].find((n) => n.className.includes("dataBlob"));
    blob?.click();
  });
  await new Promise((r) => setTimeout(r, 200));
});

// The empty state, before any Source is chosen.
await shot("empty_b", `${BASE}/?variant=B&fixture=none/missing`);

await browser.close();
