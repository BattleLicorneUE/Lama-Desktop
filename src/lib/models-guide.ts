/**
 * Guide des modèles Ollama Cloud : usage conseillé, coût, contexte.
 * Les prix (en dollars par million de jetons) et les niveaux d'usage viennent d'ollama.com/pricing et ollama.com/search?c=cloud ;
 * ils changent : la date du relevé est affichée à côté, avec un lien vers la source.
 */

export const GUIDE_DATE = "24/10/2026";
export const PRICING_URL = "https://ollama.com/pricing";
export const MODELS_URL = "https://ollama.com/search?c=cloud";

/** Niveau d'usage annoncé par Ollama : plus il est haut, plus le modèle consomme vite tes crédits. */
export type Band = "low" | "medium" | "high" | "xhigh";
export type Use = "daily" | "code" | "vision" | "reason" | "long";

export type GuideModel = {
  id: string;
  maker: string;
  blurb: string;
  band: Band;
  /** Dollars par million de jetons envoyés / générés ; `null` : non indiqué ou gratuit. */
  in: number | null;
  out: number | null;
  ctx: string;
  caps: ("vision" | "tools" | "thinking")[];
  uses: Use[];
  /** Phrase de conseil, quand le modèle est un bon choix pour un cas précis. */
  pick?: string;
  free?: boolean;
};

export const USES: { id: Use; label: string; hint: string }[] = [
  { id: "daily", label: "Au quotidien", hint: "Discuter, écrire, résumer, traduire" },
  { id: "code", label: "Code et agents", hint: "Programmer, corriger, enchaîner des outils" },
  { id: "vision", label: "Images", hint: "Lire une capture, un schéma, une photo" },
  { id: "reason", label: "Raisonnement", hint: "Problèmes difficiles, analyses longues" },
  { id: "long", label: "Longs documents", hint: "Contexte de 500 000 jetons ou plus" },
];

export const BANDS: Record<Band, { label: string; hint: string }> = {
  low: { label: "Usage faible", hint: "Ménage tes crédits : idéal pour tout essayer." },
  medium: { label: "Usage moyen", hint: "Bon équilibre entre qualité et consommation." },
  high: { label: "Usage élevé", hint: "Les crédits partent plus vite." },
  xhigh: { label: "Usage très élevé", hint: "À réserver aux tâches qui le justifient." },
};

export const GUIDE: GuideModel[] = [
  { id: "gemma4", maker: "Google", blurb: "Petit, rapide et très économique. Comprend les images et plusieurs langues.", band: "low", in: 0.14, out: 0.4, ctx: "256K", caps: ["vision", "tools", "thinking"], uses: ["daily", "vision"], pick: "Pour débuter" },
  { id: "nemotron-3-nano", maker: "NVIDIA", blurb: "Le moins cher de la liste, pensé pour les agents légers.", band: "low", in: 0.06, out: 0.24, ctx: "256K", caps: ["tools", "thinking"], uses: ["daily", "code"] },
  { id: "gpt-oss:20b", maker: "OpenAI", blurb: "Modèle ouvert compact, réactif, bon pour les échanges courants.", band: "low", in: 0.07, out: 0.3, ctx: "128K", caps: ["tools", "thinking"], uses: ["daily"] },
  { id: "gpt-oss:120b", maker: "OpenAI", blurb: "Plus solide que la version 20b pour un prix encore bas : un bon généraliste.", band: "medium", in: 0.15, out: 0.6, ctx: "128K", caps: ["tools", "thinking"], uses: ["daily", "reason"], pick: "Généraliste économique" },
  { id: "beam", maker: "Reflection", blurb: "Modèle ouvert sobre en jetons, annoncé gratuit sur ollama.com, avec 1M de contexte.", band: "low", in: null, out: null, ctx: "1M", caps: ["tools", "thinking"], uses: ["daily", "code", "long"], free: true },
  { id: "glm-5.3-flash", maker: "Z.ai", blurb: "Multimodal, 1M de contexte, bon en code et en agents pour très peu cher.", band: "medium", in: 0.15, out: 0.5, ctx: "1M", caps: ["vision", "tools", "thinking"], uses: ["daily", "code", "vision", "long"], pick: "Meilleur équilibre" },
  { id: "minimax-m3.1-flash", maker: "MiniMax", blurb: "Version rapide pour le code, les agents et le pilotage d'interfaces.", band: "medium", in: 0.15, out: 0.6, ctx: "976K", caps: ["vision", "tools", "thinking"], uses: ["code", "vision", "long"] },
  { id: "deepseek-v4.1-flash", maker: "DeepSeek", blurb: "Rapide, 1M de contexte, bonne capacité pour son prix.", band: "medium", in: 0.3, out: 1.2, ctx: "1M", caps: ["vision", "tools", "thinking"], uses: ["daily", "vision", "long"] },
  { id: "nemotron-3-super", maker: "NVIDIA", blurb: "Efficace pour les agents qui enchaînent beaucoup d'étapes.", band: "medium", in: 0.015, out: 0.6, ctx: "256K", caps: ["tools", "thinking"], uses: ["code", "reason"] },
  { id: "minimax-m2.7", maker: "MiniMax", blurb: "Code et productivité professionnelle, contexte de taille moyenne.", band: "medium", in: 0.3, out: 1.2, ctx: "192K", caps: ["tools", "thinking"], uses: ["code"] },
  { id: "mistral-large-3", maker: "Mistral", blurb: "Généraliste multimodal d'une société française, à l'aise en français.", band: "medium", in: 0.5, out: 1.5, ctx: "256K", caps: ["vision", "tools"], uses: ["daily", "vision"] },
  { id: "mistral-large-4", maker: "Mistral", blurb: "Évolution du précédent : 1M de contexte et réflexion.", band: "high", in: 0.68, out: 2.09, ctx: "1M", caps: ["vision", "tools", "thinking"], uses: ["daily", "vision", "long"] },
  { id: "minimax-m3", maker: "MiniMax", blurb: "Code et agents de haut niveau, multimodal.", band: "high", in: 0.6, out: 2.4, ctx: "500K", caps: ["vision", "tools", "thinking"], uses: ["code", "vision", "long"] },
  { id: "kimi-k2.7-code", maker: "Moonshot", blurb: "Spécialisé dans les longues tâches de programmation.", band: "high", in: 0.95, out: 4, ctx: "256K", caps: ["vision", "tools", "thinking"], uses: ["code", "vision"], pick: "Code" },
  { id: "kimi-k2.6", maker: "Moonshot", blurb: "Agent multimodal pour le code et l'exécution autonome.", band: "high", in: 0.95, out: 4, ctx: "256K", caps: ["vision", "tools", "thinking"], uses: ["code", "vision"] },
  { id: "muse-spark-open", maker: "Meta", blurb: "Gros modèle multimodal pour le génie logiciel et les tâches longues.", band: "high", in: 1.25, out: 4.25, ctx: "1M", caps: ["vision", "tools", "thinking"], uses: ["code", "vision", "reason", "long"] },
  { id: "nemotron-3-ultra", maker: "NVIDIA", blurb: "Raisonnement et agents de longue durée, entrée très peu chère.", band: "high", in: 0.1, out: 3, ctx: "256K", caps: ["tools", "thinking"], uses: ["reason", "code"] },
  { id: "glm-5.2", maker: "Z.ai", blurb: "Modèle phare pour les tâches longues et le code.", band: "high", in: 1.4, out: 4.4, ctx: "1M", caps: ["tools", "thinking"], uses: ["code", "reason", "long"] },
  { id: "glm-5.3", maker: "Z.ai", blurb: "Le plus capable en code parmi les modèles ouverts, selon Ollama.", band: "high", in: 1.4, out: 4.4, ctx: "1M", caps: ["tools", "thinking"], uses: ["code", "reason", "long"], pick: "Code exigeant" },
  { id: "deepseek-v4-pro", maker: "DeepSeek", blurb: "Très grand modèle avec trois modes de réflexion.", band: "xhigh", in: 1.32, out: 3.96, ctx: "1M", caps: ["tools", "thinking"], uses: ["reason", "code", "long"] },
  { id: "kimi-k3", maker: "Moonshot", blurb: "Le plus puissant de la liste, et le plus cher.", band: "xhigh", in: 3, out: 15, ctx: "1M", caps: ["vision", "tools", "thinking"], uses: ["reason", "code", "vision", "long"], pick: "Le plus puissant" },
];

/** Retrouve la fiche d'un modèle : nom exact, sans le suffixe « -cloud », puis sans la taille (« gemma4:31b » → « gemma4 »). */
export function guideFor(name: string): GuideModel | undefined {
  const n = name.toLowerCase().replace(/^cloud\//, "").replace(/-cloud$/, "");
  return GUIDE.find((g) => g.id === n) ?? GUIDE.find((g) => g.id === n.split(":")[0]);
}

/** Symboles de prix selon le coût des jetons générés : $ très bon marché … $$$$ cher. */
export function priceTier(m: Pick<GuideModel, "out" | "free">): { symbols: string; label: string } {
  if (m.free) return { symbols: "Gratuit", label: "Gratuit d'après ollama.com" };
  const out = m.out ?? 0;
  if (out <= 0.6) return { symbols: "$", label: "Très économique" };
  if (out <= 2.5) return { symbols: "$$", label: "Abordable" };
  if (out <= 5) return { symbols: "$$$", label: "Cher" };
  return { symbols: "$$$$", label: "Très cher" };
}

const money = (n: number) => (n < 0.1 ? n.toFixed(3).replace(/0$/, "") : n.toFixed(2)).replace(".", ",");

/** « 0,15 $ → 0,60 $ par million de jetons » */
export function priceText(m: Pick<GuideModel, "in" | "out" | "free">): string {
  if (m.free) return "Gratuit d'après ollama.com";
  if (m.in == null || m.out == null) return "Prix non indiqué";
  return `${money(m.in)} $ envoyés · ${money(m.out)} $ générés (par million de jetons)`;
}

const BAND_ORDER: Band[] = ["low", "medium", "high", "xhigh"];

/** Du moins cher au plus cher (jetons générés), à usage égal. */
export function byPrice(a: GuideModel, b: GuideModel): number {
  return (a.free ? -1 : (a.out ?? 0)) - (b.free ? -1 : (b.out ?? 0)) || BAND_ORDER.indexOf(a.band) - BAND_ORDER.indexOf(b.band);
}

/** Les modèles utiles à un usage, du plus économique au plus cher. */
export function forUse(use: Use | "all"): GuideModel[] {
  return GUIDE.filter((g) => use === "all" || g.uses.includes(use)).sort(byPrice);
}
