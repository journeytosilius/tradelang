#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

function usage() {
    console.error(
        "usage: node scripts/install-server-bundle.mjs --platform <platform-arch> --binary <path>",
    );
    process.exit(1);
}

const args = process.argv.slice(2);
let platform = "";
let binary = "";

for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--platform") {
        platform = args[index + 1] ?? "";
        index += 1;
    } else if (arg === "--binary") {
        binary = args[index + 1] ?? "";
        index += 1;
    } else {
        usage();
    }
}

if (platform.length === 0 || binary.length === 0) {
    usage();
}

const source = path.resolve(binary);
if (!fs.existsSync(source)) {
    console.error(`binary not found: ${source}`);
    process.exit(1);
}

const fileName = platform.startsWith("win32-") ? "palmscript-lsp.exe" : "palmscript-lsp";
const destinationDir = path.resolve("server", platform);
const destination = path.join(destinationDir, fileName);

fs.mkdirSync(destinationDir, { recursive: true });
fs.copyFileSync(source, destination);
if (!platform.startsWith("win32-")) {
    fs.chmodSync(destination, 0o755);
}

console.log(`installed ${source} -> ${destination}`);
