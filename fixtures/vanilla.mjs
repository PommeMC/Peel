// Runs the vanilla client installed by the Minecraft launcher, for capturing
// reference traces: `client <version>` joins the capture proxy as `Player`,
// and `java <version>` prints the Java runtime that version needs.
// MINECRAFT_DIR and JAVA override where they are found.

import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const os = { win32: "windows", darwin: "osx" }[process.platform] ?? "linux";
const arch = process.arch === "ia32" ? "x86" : process.arch;
const minecraftDir =
  process.env.MINECRAFT_DIR ??
  {
    windows: path.join(process.env.APPDATA ?? homedir(), ".minecraft"),
    osx: path.join(homedir(), "Library/Application Support/minecraft"),
    linux: path.join(homedir(), ".minecraft"),
  }[os];

const [command, version] = process.argv.slice(2);
const versionDir = path.join(minecraftDir, "versions", version ?? "");
const commands = { client, java: (json) => console.log(java(json)) };
if (!commands[command] || !version) {
  fail("usage: node fixtures/vanilla.mjs <client|java> <version>");
}
commands[command](versionJson());

function fail(message) {
  console.error(`vanilla: ${message}`);
  process.exit(1);
}

function requireInstalled(file) {
  if (!existsSync(file)) {
    fail(
      `${file} is missing; play ${version} once from the Minecraft launcher`,
    );
  }
}

function versionJson() {
  const file = path.join(versionDir, `${version}.json`);
  requireInstalled(file);
  return JSON.parse(readFileSync(file, "utf8"));
}

/** Whether the launcher's `rules` allow something on this machine. */
function allowed(rules = [{ action: "allow" }]) {
  let allow = false;
  for (const rule of rules) {
    const matches =
      !rule.features &&
      (!rule.os?.name || rule.os.name === os) &&
      (!rule.os?.arch || rule.os.arch === arch);
    if (matches) allow = rule.action === "allow";
  }
  return allow;
}

/** The launcher's own runtime for the version, where it installed one. */
function java(json) {
  if (process.env.JAVA) return process.env.JAVA;
  const { component, majorVersion } = json.javaVersion;
  const platform = {
    windows: `windows-${arch}`,
    osx: arch === "arm64" ? "mac-os-arm64" : "mac-os",
    linux: "linux",
  }[os];
  const runtimes = [path.join(minecraftDir, "runtime")];
  if (os === "windows") {
    // The Microsoft Store launcher keeps its runtimes in its package.
    runtimes.push(
      path.join(
        process.env.LOCALAPPDATA ?? "",
        "Packages/Microsoft.4297127D64EC6_8wekyb3d8bbwe/LocalCache/Local/runtime",
      ),
    );
  }
  const exe = os === "windows" ? "java.exe" : "java";
  const found = runtimes
    .map((dir) => path.join(dir, component, platform, component, "bin", exe))
    .find(existsSync);
  return (
    found ??
    fail(`no ${component} runtime found; set JAVA to Java ${majorVersion}`)
  );
}

function client(json) {
  const libraries = path.join(minecraftDir, "libraries");
  const classpath = json.libraries
    .filter((library) => allowed(library.rules))
    .map((library) => path.join(libraries, library.downloads.artifact.path))
    .concat(path.join(versionDir, `${version}.jar`));
  classpath.forEach(requireInstalled);

  const gameDir = path.resolve(
    import.meta.dirname,
    "../target/fixture-client",
    version,
  );
  mkdirSync(gameDir, { recursive: true });
  // Replacing the whole file resets every other option to vanilla's default.
  copyFileSync(
    path.join(import.meta.dirname, "options.txt"),
    path.join(gameDir, "options.txt"),
  );

  const values = {
    natives_directory: path.join(gameDir, "natives"),
    launcher_name: "peel",
    launcher_version: "0",
    classpath: classpath.join(path.delimiter),
  };
  const jvm = json.arguments.jvm
    .flatMap((arg) =>
      typeof arg === "string" ? arg : allowed(arg.rules) ? arg.value : [],
    )
    .map((arg) => arg.replace(/\$\{(\w+)\}/g, (_, key) => values[key]));
  const game = Object.entries({
    username: "Player",
    accessToken: "0",
    version,
    gameDir,
    assetsDir: path.join(minecraftDir, "assets"),
    assetIndex: json.assetIndex.id,
    quickPlayMultiplayer: "127.0.0.1:25566",
  }).flatMap(([key, value]) => [`--${key}`, value]);
  const args = [
    "-Xmx2G",
    ...jvm,
    json.mainClass,
    ...game,
    "--offlineDeveloperMode",
  ];
  spawn(java(json), args, { stdio: "inherit" }).on("exit", (code) =>
    process.exit(code ?? 1),
  );
}
