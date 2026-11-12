// Downloads the `vyrn` program for this computer from the GitHub release
// that matches this package's version, into ./bin. Runs after
// `npm install`; `npx vyrn-lang` runs it too.
//
//   VYRN_BINARY_PATH=/path/to/vyrn   use a vyrn you already have (no download)
//   VYRN_BINARY_URL=https://...      download from somewhere else

"use strict";
const fs = require("fs");
const https = require("https");
const path = require("path");

const REPO = "dmccottrell/vyrn";
const version = "v" + require("./package.json").version;

/** The Rust target name of this computer, as the releases are named. */
function target() {
  const arch = { x64: "x86_64", arm64: "aarch64" }[process.arch];
  const os = { linux: "unknown-linux-gnu", darwin: "apple-darwin", win32: "pc-windows-msvc" }[process.platform];
  if (!arch || !os || (process.platform === "win32" && arch !== "x86_64")) {
    throw new Error(
      `there is no prebuilt vyrn for ${process.platform} ${process.arch}; ` +
        `build it with: cargo install --git https://github.com/${REPO}`
    );
  }
  return `${arch}-${os}`;
}

function binaryPath() {
  return path.join(__dirname, "bin", process.platform === "win32" ? "vyrn.exe" : "vyrn-native");
}

/** Fetches a URL, following GitHub's redirects to its download servers. */
function download(url, dest, redirects = 5) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "vyrn-npm-installer" } }, (res) => {
        if ([301, 302, 303, 307, 308].includes(res.statusCode) && res.headers.location && redirects > 0) {
          res.resume();
          resolve(download(new URL(res.headers.location, url).toString(), dest, redirects - 1));
          return;
        }
        if (res.statusCode !== 200) {
          res.resume();
          reject(new Error(`downloading ${url} failed: HTTP ${res.statusCode}`));
          return;
        }
        const tmp = dest + ".part";
        const file = fs.createWriteStream(tmp);
        res.pipe(file);
        file.on("finish", () => file.close(() => fs.rename(tmp, dest, (e) => (e ? reject(e) : resolve()))));
        file.on("error", reject);
      })
      .on("error", reject);
  });
}

async function install() {
  const dest = binaryPath();
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  if (process.env.VYRN_BINARY_PATH) {
    fs.copyFileSync(process.env.VYRN_BINARY_PATH, dest);
  } else {
    const exe = process.platform === "win32" ? ".exe" : "";
    const url =
      process.env.VYRN_BINARY_URL ||
      `https://github.com/${REPO}/releases/download/${version}/vyrn-${version}-${target()}${exe}`;
    console.log(`vyrn: downloading ${url}`);
    await download(url, dest);
  }
  if (process.platform !== "win32") fs.chmodSync(dest, 0o755);
  console.log(`vyrn: installed ${dest}`);
}

module.exports = { install, binaryPath, target };

if (require.main === module) {
  install().catch((e) => {
    console.error(`vyrn: ${e.message}`);
    process.exit(1);
  });
}
