import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

/**
 * Notification Windows, seulement si l'application n'est pas au premier plan (sinon l'utilisateur voit déjà le résultat).
 * Le choix se fait côté système, donc aussi quand la fenêtre est réduite ou cachée dans la zone de notification.
 */
export async function notify(title: string, body: string) {
  try {
    await invoke("notify_os", { title, body });
    return;
  } catch {
    // Hors application (aperçu navigateur) : repli sur le plugin côté interface.
  }
  if (typeof document !== "undefined" && document.hasFocus()) return;
  try {
    let ok = await isPermissionGranted();
    if (!ok) ok = (await requestPermission()) === "granted";
    if (ok) sendNotification({ title, body: body.slice(0, 140) });
  } catch {
    // Pas de notification possible : sans importance.
  }
}
