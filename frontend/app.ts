const api = (path: string) => fetch(path);

async function refresh(): Promise<void> {
  const snap = await api("/api/snapshot").then((r) => r.json());
  const badge = document.getElementById("badge");
  if (badge) badge.textContent = snap.delayed_badge ?? "DELAYED";
  const overview = document.getElementById("overview-body");
  if (overview) overview.textContent = JSON.stringify(snap, null, 2);
  const top = await api("/api/top20").then((r) => r.json());
  const chain = document.getElementById("chain-body");
  if (chain) chain.textContent = JSON.stringify(top, null, 2);
  const blot = await api("/api/blotter").then((r) => r.json());
  const blotter = document.getElementById("blotter-body");
  if (blotter) blotter.textContent = JSON.stringify(blot, null, 2);
}

document.getElementById("kill")?.addEventListener("click", async () => {
  await fetch("/api/kill", { method: "POST" });
  await refresh();
});

void refresh();
