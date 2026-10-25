/** Boîte de confirmation de l'application (remplace `confirm()` du navigateur, qui ne suit pas le thème). */
type Ask = { message: string; ok: string; resolve: (yes: boolean) => void };

export const confirmState = $state<{ current: Ask | null }>({ current: null });

export function askConfirm(message: string, ok = "Supprimer"): Promise<boolean> {
  return new Promise((resolve) => {
    confirmState.current?.resolve(false);
    confirmState.current = { message, ok, resolve };
  });
}

export function answerConfirm(yes: boolean) {
  confirmState.current?.resolve(yes);
  confirmState.current = null;
}
