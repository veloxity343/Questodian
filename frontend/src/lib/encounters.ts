import { api } from "../api";
import type { Encounter } from "../types";

/** Where "Add to encounter" goes outside the DM screen: the running encounter, else the last edited. */
export async function targetEncounter(campaignId: string): Promise<Encounter | null> {
  if (!campaignId) return null;
  const list = await api.list("encounters", { campaign_id: campaignId });
  return list.find((e) => e.active) ?? [...list].sort((a, b) => b.updated.localeCompare(a.updated))[0] ?? null;
}
