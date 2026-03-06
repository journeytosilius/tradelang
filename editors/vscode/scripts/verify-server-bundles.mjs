#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const requiredBundles = [
    { platform: "linux-x64", binary: "palmscript-lsp" },
    { platform: "darwin-x64", binary: "palmscript-lsp" },
    { platform: "darwin-arm64", binary: "palmscript-lsp" },
    { platform: "win32-x64", binary: "palmscript-lsp.exe" },
];

const missing = [];
for (const bundle of requiredBundles) {
    const candidate = path.resolve("server", bundle.platform, bundle.binary);
    if (!fs.existsSync(candidate)) {
        missing.push(candidate);
    }
}

if (missing.length > 0) {
    console.error("missing bundled palmscript-lsp binaries:");
    for (const candidate of missing) {
        console.error(`  - ${candidate}`);
    }
    process.exit(1);
}

console.log("all required palmscript-lsp bundles are present");
