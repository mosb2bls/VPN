#!/usr/bin/env node
// Download the pinned rule-sets listed in resources/rules/manifest.json into
// resources/rules/ (gitignored) and verify size, git blob hash and SHA-256.
// Safe to re-run. Downloads only from raw.githubusercontent.com/SagerNet/*.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync, renameSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dir = join(root, "resources", "rules");
const manifest = JSON.parse(readFileSync(join(dir, "manifest.json"), "utf8"));

const sha256 = (b) => createHash("sha256").update(b).digest("hex");
const gitBlob = (b) => createHash("sha1").update(`blob ${b.length}\0`).update(b).digest("hex");

function verify(set, buf) {
  if (buf.length !== set.size) return `size ${buf.length} != ${set.size}`;
  if (gitBlob(buf) !== set.gitBlob) return "git blob hash mismatch";
  if (sha256(buf) !== set.sha256) return "SHA-256 mismatch";
  return null;
}

try {
  for (const set of manifest.sets) {
    const dest = join(dir, set.path);
    if (existsSync(dest) && verify(set, readFileSync(dest)) === null) {
      console.log(`[fetch-rules] ${set.name}: already present and verified`);
      continue;
    }
    if (!/^SagerNet\/[\w.-]+$/.test(set.repo) || !/^[0-9a-f]{40}$/.test(set.commit)) {
      throw new Error(`${set.name}: unexpected repo/commit in manifest`);
    }
    const url = `https://raw.githubusercontent.com/${set.repo}/${set.commit}/${set.path}`;
    console.log(`[fetch-rules] ${set.name}: downloading ${set.repo}@${set.commit.slice(0, 7)}`);
    const res = await fetch(url);
    if (!res.ok) throw new Error(`${set.name}: HTTP ${res.status}`);
    const buf = Buffer.from(await res.arrayBuffer());
    const problem = verify(set, buf);
    if (problem) throw new Error(`${set.name}: ${problem}. Refusing to install.`);
    writeFileSync(dest + ".tmp", buf);
    renameSync(dest + ".tmp", dest);
    console.log(`[fetch-rules] ${set.name}: verified and installed (${buf.length} bytes)`);
  }
} catch (e) {
  console.error(`[fetch-rules] ERROR: ${e.message}`);
  process.exit(1);
}
