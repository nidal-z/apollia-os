/**
 * Prime the microphone permission via a one-shot `getUserMedia` request.
 *
 * The live waveform and the transcription both come from the Rust cpal capture
 * stream, not from the WebView. But on macOS the microphone TCC grant is
 * per-application: nothing in the native capture path raises the system
 * permission prompt on its own, so on a fresh install cpal reads silence (flat
 * waveform, empty transcription) until the app has been granted mic access.
 *
 * A brief `getUserMedia({ audio: true })` from the WebView raises that prompt
 * and, once granted, the whole app bundle (including the cpal stream) can read
 * the microphone. The returned tracks are stopped immediately: this request
 * exists only to obtain the grant, never to capture audio.
 *
 * Safe to call repeatedly: once granted the browser resolves without a prompt.
 * Returns `true` when the microphone is accessible, `false` otherwise (denied,
 * no device, or unsupported environment). Never throws.
 */
export async function ensureMicPermission(): Promise<boolean> {
  if (typeof navigator === "undefined" || !navigator.mediaDevices?.getUserMedia) {
    return false;
  }
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    for (const track of stream.getTracks()) {
      track.stop();
    }
    return true;
  } catch {
    return false;
  }
}
