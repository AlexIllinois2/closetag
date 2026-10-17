const B = "http://127.0.0.1:17744";
const post = async (p: string, body?: unknown) => (await fetch(B + p, { method: "POST", headers: { "Content-Type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) })).json();
const repo = await post("/api/repos", { path: "C:/Users/45574/AppData/Local/Temp/ct-bug/repo" });
await post(`/api/repos/${repo.id}/scan`);
console.log("fixture:", repo.id);
