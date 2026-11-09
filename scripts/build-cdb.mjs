import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, basename } from "node:path";

const HEADER_BYTES = 16;
const RECORD_BYTES = 40;
const DAY_MS = 24 * 60 * 60 * 1000;

function option(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : process.argv[index + 1];
}

function fail(message) {
  throw new Error(message);
}

function epochDay(date) {
  const millis = Date.parse(date + "T00:00:00Z");
  if (!Number.isFinite(millis)) {
    fail("Invalid UTC date: " + date);
  }
  return Math.floor(millis / DAY_MS);
}

function dateAtDay(day) {
  return new Date(day * DAY_MS).toISOString().slice(0, 10);
}

function asUnsignedBigInt(value, field, date) {
  let integer;
  try {
    integer = BigInt(value === undefined ? 0 : value);
  } catch {
    fail("Invalid " + field + " on " + date);
  }
  if (integer < 0n || integer > 18446744073709551615n) {
    fail("Out-of-range " + field + " on " + date);
  }
  return integer;
}

function asUint32(value, field, date) {
  const integer = Number(value === undefined ? 0 : value);
  if (!Number.isInteger(integer) || integer < 0 || integer > 4294967295) {
    fail("Invalid " + field + " on " + date);
  }
  return integer;
}

const input = option("--input", "dist/data/demo.json");
const output = option("--output", "dist/data/demo.cdb");
const indexOutput = option("--index", "dist/data/demo.index.json");
if (!input || !output || !indexOutput) {
  fail("Usage: node scripts/build-cdb.mjs --input source.json --output data.cdb --index data.index.json");
}

const sourceText = await readFile(input, "utf8");
const source = input.endsWith(".ndjson")
  ? {
    days: sourceText.split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line))
  }
  : JSON.parse(sourceText);
if (!Array.isArray(source.days) || source.days.length === 0) {
  fail("Source must include at least one daily point");
}

const days = source.days.slice().sort((left, right) => String(left.date).localeCompare(String(right.date)));
const firstDay = epochDay(days[0].date);
for (let index = 0; index < days.length; index += 1) {
  const expected = dateAtDay(firstDay + index);
  if (days[index].date !== expected) {
    fail("CDB1 input must contain one contiguous UTC point per day; expected " + expected);
  }
}

const buffer = new ArrayBuffer(HEADER_BYTES + days.length * RECORD_BYTES);
const view = new DataView(buffer);
view.setUint8(0, 0x43);
view.setUint8(1, 0x44);
view.setUint8(2, 0x42);
view.setUint8(3, 0x31);
view.setUint8(4, 1);
view.setUint8(5, RECORD_BYTES);
view.setUint16(6, 0, true);
view.setUint32(8, days.length, true);
view.setInt32(12, firstDay, true);

days.forEach((day, index) => {
  const offset = HEADER_BYTES + index * RECORD_BYTES;
  const flags = asUint32(day.flags === undefined ? 3 : day.flags, "flags", day.date);
  view.setBigUint64(offset, asUnsignedBigInt(day.db_bytes, "db_bytes", day.date), true);
  view.setBigUint64(offset + 8, asUnsignedBigInt(day.wal_bytes, "wal_bytes", day.date), true);
  view.setUint32(offset + 16, asUint32(day.peak_height, "peak_height", day.date), true);
  view.setUint32(offset + 20, asUint32(day.blocks, "blocks", day.date), true);
  view.setUint32(offset + 24, asUint32(day.additions, "additions", day.date), true);
  view.setUint32(offset + 28, asUint32(day.removals, "removals", day.date), true);
  view.setUint32(offset + 32, flags, true);
  view.setUint32(offset + 36, 0, true);
});

const binary = Buffer.from(buffer);
const manifest = {
  schema_version: 1,
  format: "chia-db-daily/cdb1",
  mode: source.mode || "live",
  network: source.network || "mainnet",
  generated_at: source.generated_at || new Date().toISOString(),
  timezone: "UTC",
  data_file: basename(output),
  record_bytes: RECORD_BYTES,
  record_count: days.length,
  start_date: days[0].date,
  end_date: days[days.length - 1].date,
  sha256: createHash("sha256").update(binary).digest("hex")
};

await mkdir(dirname(output), { recursive: true });
await mkdir(dirname(indexOutput), { recursive: true });
await writeFile(output, binary);
await writeFile(indexOutput, JSON.stringify(manifest, null, 2) + "\n");
console.log("Built " + output + " with " + days.length + " daily CDB1 records.");
