/** Saves bytes the core produced as a file the user downloads. */
export function downloadBase64(fileName: string, base64: string, type: string): void {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  save(fileName, new Blob([bytes], { type }));
}

/** Saves text the core produced (XML, CSV) as a file the user downloads. */
export function downloadText(fileName: string, text: string, type: string): void {
  save(fileName, new Blob([text], { type: `${type};charset=utf-8` }));
}

function save(fileName: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  a.rel = "noopener";
  document.body.append(a);
  a.click();
  a.remove();
  // Give the browser a moment to start the download before revoking.
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
