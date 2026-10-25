/** Lecture à voix haute avec les voix de Windows (hors ligne). */

export const canSpeak = typeof window !== "undefined" && "speechSynthesis" in window;

/** Texte lisible : sans blocs de code, liens ni symboles Markdown. */
export function speakable(markdown: string): string {
  return markdown
    .replace(/```[\s\S]*?```/g, " (bloc de code) ")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/https?:\/\/\S+/g, "")
    .replace(/[#>*_~|]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

export function stopSpeaking() {
  if (canSpeak) window.speechSynthesis.cancel();
}

/** Lit le texte ; `onend` est appelé à la fin ou si la lecture est interrompue. */
export function speak(text: string, onend: () => void) {
  if (!canSpeak) return onend();
  window.speechSynthesis.cancel();
  const u = new SpeechSynthesisUtterance(speakable(text).slice(0, 8000));
  u.lang = "fr-FR";
  const voice = window.speechSynthesis.getVoices().find((v) => v.lang.toLowerCase().startsWith("fr"));
  if (voice) u.voice = voice;
  u.onend = onend;
  u.onerror = onend;
  window.speechSynthesis.speak(u);
}
