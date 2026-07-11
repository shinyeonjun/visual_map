export async function copyValue(value: string): Promise<boolean> {
  try {
    const isTauri = "__TAURI_INTERNALS__" in window;
    if (!isTauri && navigator.clipboard?.writeText) {
      await Promise.race([
        navigator.clipboard.writeText(value),
        new Promise<never>((_, reject) =>
          window.setTimeout(() => reject(new Error("Clipboard API timed out")), 750),
        ),
      ]);
      return true;
    }
  } catch {
    // Fall through to the WebView-compatible selection fallback.
  }

  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  return copied;
}
