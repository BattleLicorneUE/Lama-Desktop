import { invoke } from "@tauri-apps/api/core";

export type Sent = { t: number; name: string; props: Record<string, unknown> };
export type AnalyticsInfo = {
  available: boolean;
  asked: boolean;
  enabled: boolean;
  events: [string, string][];
  sent: Sent[];
};

class Analytics {
  info = $state<AnalyticsInfo | null>(null);

  async load() {
    try {
      this.info = await invoke<AnalyticsInfo>("analytics_get");
    } catch {
      this.info = null;
    }
  }

  async set(enabled: boolean) {
    this.info = await invoke<AnalyticsInfo>("analytics_set", { enabled });
  }
}

export const analytics = new Analytics();
