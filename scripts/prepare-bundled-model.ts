import { createHash } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const catalog = JSON.parse(
  await readFile(
    new URL("../src-tauri/src/catalog/catalog.json", import.meta.url),
    "utf8",
  ),
);
const model = catalog.models.find(
  (entry: { id: string }) => entry.id === "SenseVoiceSmall",
);
const file = model.files.Q5_K_M;
const destination = resolve(
  import.meta.dir,
  "../src-tauri/resources/models",
  file.filename,
);

function verify(bytes: Uint8Array) {
  if (
    bytes.length !== file.size ||
    createHash("sha256").update(bytes).digest("hex") !== file.sha256
  ) {
    throw new Error(
      "Bundled SenseVoice Q5 failed catalog integrity verification",
    );
  }
}

let bytes: Buffer | undefined;
try {
  bytes = await readFile(destination);
} catch (error) {
  if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
}
if (bytes) {
  verify(bytes);
} else {
  const url = `https://huggingface.co/${model.repository}/resolve/${model.revision}/${file.filename}`;
  console.log(`Fetching build-time bundled model: ${url}`);
  const response = await fetch(url);
  if (!response.ok)
    throw new Error(`Model download failed: ${response.status}`);
  bytes = Buffer.from(await response.arrayBuffer());
  verify(bytes);
  await mkdir(dirname(destination), { recursive: true });
  await writeFile(`${destination}.partial`, bytes);
  await rename(`${destination}.partial`, destination);
}
console.log(
  `Bundled SenseVoice Q5 verified: ${file.filename}, ${file.size} bytes`,
);
