import { useNavigate } from "react-router-dom";
import { api } from "../api";
import { EmptyState, LoadError, Loading } from "../components/ui";
import { useApp } from "../context/app";
import { useDialogs } from "../context/ui";
import { useLoad } from "../lib/hooks";
import { plural } from "../lib/util";
import type { Edition } from "../types";

/** Pick or create a campaign; the DM screen then works on it. */
export function CampaignList() {
  const { campaignId, setCampaignId, refreshCampaigns, meta } = useApp();
  const { prompt } = useDialogs();
  const navigate = useNavigate();
  const { data, error } = useLoad(() => Promise.all([api.list("campaigns"), api.list("sessions"), api.list("scenes")]), []);
  if (error) return <LoadError error={error} />;
  if (!data) return <Loading />;
  const [list, sessions, scenes] = data;
  const editions = meta?.editions ?? { "2014": "5e (2014)", "2024": "5.5e (2024)" };

  const choose = (id: string) => {
    setCampaignId(id);
    navigate("/");
  };
  const create = async (edition: Edition) => {
    const name = await prompt(`New campaign (${editions[edition]})`, { label: "Name", value: "" });
    if (name === null) return;
    const system = edition === "2024" ? "5e 2024 (SRD 5.2.1)" : "5e (SRD 5.1)";
    const c = await api.create("campaigns", { name: name || "New Campaign", edition, system });
    await refreshCampaigns();
    choose(c.id);
  };

  const buttons = (
    <div className="row gap wrap">
      <button type="button" className="btn primary" onClick={() => create("2024")}>+ New campaign · 2024 rules</button>
      <button type="button" className="btn" onClick={() => create("2014")}>+ New campaign · 2014 rules</button>
    </div>
  );

  return (
    <div className="page">
      <header className="page-head"><h1>Campaigns</h1>{buttons}</header>
      {list.length ? (
        <div className="cards">
          {list.map((c) => {
            const ss = sessions.filter((s) => s.campaign_id === c.id);
            const next = ss.find((s) => s.status !== "done");
            return (
              <button type="button" key={c.id} className={`card campaign-card ${c.id === campaignId ? "selected" : ""}`} onClick={() => choose(c.id)}>
                <span className="row gap"><h3 className="grow">{c.name}</h3><span className="tag">{editions[c.edition]}</span></span>
                <p className="muted clamp">{c.description || c.setting || "No description yet."}</p>
                <span className="card-meta">
                  <span>{plural(ss.length, "session")}</span>
                  <span>{plural(scenes.filter((s) => s.campaign_id === c.id).length, "scene")}</span>
                  {next && <span className="tag">Next: {next.number}. {next.title}</span>}
                </span>
              </button>
            );
          })}
        </div>
      ) : (
        <EmptyState title="Start your first campaign" text="A campaign keeps your sessions, scenes, NPCs, encounters and notes together.">
          {buttons}
        </EmptyState>
      )}
    </div>
  );
}
