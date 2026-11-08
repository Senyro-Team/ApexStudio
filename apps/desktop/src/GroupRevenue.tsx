import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  desktopAvailable,
  errorMessage,
  importRobloxGroupSales,
  groupRevenueBrowserStatus,
  startGroupRevenueBrowser,
  removeRobloxGroupRevenue,
  robloxGroupRevenue,
  saveRobloxRevenueGroup,
  statisticsProfile,
  statisticsSales,
  type RobloxGroupRevenueData,
  type RobloxGroupSale,
  type RobloxRevenueGroup,
  type SalesSnapshot,
} from "./bridge";
import "./groupRevenue.css";

const number = (value: number) => new Intl.NumberFormat().format(value);
const robux = (value: number) => `R$ ${number(value)}`;
const idPattern = /^[1-9]\d{0,19}$/;

export function parseCsv(source: string): string[][] {
  const text = source.replace(/^\uFEFF/, "");
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (quoted) {
      if (char === '"' && text[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (char === '"') quoted = false;
      else cell += char;
    } else if (char === '"' && cell.length === 0) quoted = true;
    else if (char === ",") {
      row.push(cell);
      cell = "";
    } else if (char === "\n" || char === "\r") {
      if (char === "\r" && text[i + 1] === "\n") i++;
      row.push(cell);
      if (row.some((part) => part.trim() !== "")) rows.push(row);
      row = [];
      cell = "";
    } else cell += char;
  }
  if (quoted) throw new Error("The CSV contains an unclosed quoted field.");
  row.push(cell);
  if (row.some((part) => part.trim() !== "")) rows.push(row);
  if (rows.length < 2) throw new Error("The CSV has no sales rows.");
  return rows;
}

const normalizeHeader = (value: string) =>
  value.trim().toLocaleLowerCase().normalize("NFKD").replace(/[^\p{L}\p{N}]/gu, "");

function readField(row: string[], columns: Map<string, number>, name: string) {
  const index = columns.get(name);
  return index === undefined ? "" : (row[index] ?? "").trim();
}

export function amount(value: string, label: string) {
  const clean = value.trim().replace(/^R\$\s*/i, "").replace(/,/g, "");
  if (!/^\d+(?:\.0+)?$/.test(clean)) throw new Error(`${label} must be a whole number of Robux.`);
  const parsed = Number(clean);
  if (!Number.isSafeInteger(parsed)) throw new Error(`${label} is outside the supported range.`);
  return parsed;
}

export type ParsedPreview = {
  rows: RobloxGroupSale[];
  skipped: number;
  files: string[];
  warnings: string[];
  from: string;
  to: string;
};

async function digest(text: string) {
  const bytes = new TextEncoder().encode(text);
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(hash), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function parseDate(value: string) {
  const parsed = new Date(value);
  if (!Number.isFinite(parsed.getTime())) throw new Error("A sale row has an invalid UTC date.");
  return parsed.toISOString();
}

export async function readCsvFiles(files: File[], group: RobloxRevenueGroup): Promise<ParsedPreview> {
  const experiences = new Set(group.experienceIds);
  const rows: Omit<RobloxGroupSale, "saleFingerprint">[] = [];
  let skipped = 0;
  const warnings: string[] = [];
  for (const file of files) {
    if (file.size > 25 * 1024 * 1024) throw new Error(`${file.name} is larger than 25 MB.`);
    const parsed = parseCsv(await file.text());
    const columns = new Map(parsed[0].map((header, index) => [normalizeHeader(header), index]));
    const aliases: Record<string, string[]> = {
      saleDate: ["saledateandtime", "dateandtime"],
      saleLocation: ["salelocation", "location"],
      universeId: ["universeid"], assetId: ["assetid"], assetType: ["assettype"],
      holdStatus: ["holdstatus"], revenue: ["revenue"], price: ["price"],
    };
    const column = (field: keyof typeof aliases) => aliases[field].find((alias) => columns.has(alias));
    const required = ["saleDate", "universeId", "assetId", "assetType", "holdStatus", "revenue", "price"] as const;
    const missing = required.filter((field) => !column(field));
    if (missing.length) throw new Error(`${file.name} is missing required Roblox CSV columns: ${missing.join(", ")}.`);
    const recognized = new Set([...Object.values(aliases).flat(), "id", "buyeruserid", "universe", "assetname", "locationid"]);
    const unexpected = [...columns.keys()].filter((header) => !recognized.has(header));
    if (unexpected.length) warnings.push(`${file.name}: ignored unrecognized column(s): ${unexpected.join(", ")}.`);
    for (let line = 1; line < parsed.length; line++) {
      const row = parsed[line];
      const type = normalizeHeader(readField(row, columns, column("assetType")!));
      const universeId = readField(row, columns, column("universeId")!);
      if (type !== "gamepass" || !experiences.has(universeId)) {
        skipped++;
        continue;
      }
      const assetId = readField(row, columns, column("assetId")!);
      if (!idPattern.test(universeId) || !idPattern.test(assetId)) throw new Error(`${file.name}, row ${line + 1}: invalid experience or Game Pass asset ID.`);
      const holdValue = normalizeHeader(readField(row, columns, column("holdStatus")!));
      const holdStatus = holdValue === "released" ? "Released" : holdValue === "held" ? "Held" : holdValue === "cancelled" || holdValue === "canceled" ? "Cancelled" : null;
      if (!holdStatus) throw new Error(`${file.name}, row ${line + 1}: unknown hold status.`);
      rows.push({
        universeId,
        assetId,
        saleAt: parseDate(readField(row, columns, column("saleDate")!)),
        saleLocation: column("saleLocation") ? readField(row, columns, column("saleLocation")!) : "",
        holdStatus,
        revenue: amount(readField(row, columns, "revenue"), "Revenue"),
        price: amount(readField(row, columns, "price"), "Price"),
        importBatchId: "",
        externalTransactionId: columns.has("id") ? readField(row, columns, "id") || null : null,
        source: "csv",
        dedupeConfidence: columns.has("id") ? "transaction_id" : "exact_fields",
      });
    }
  }
  if (!rows.length) throw new Error("No Game Pass sales for the configured experiences were found in these CSV files.");

  // Never persist Buyer User ID. Stable Roblox IDs are the preferred idempotency key.
  const occurrences = new Map<string, number>();
  const batchId = crypto.randomUUID();
  const sales: RobloxGroupSale[] = [];
  for (const row of rows) {
    const signature = JSON.stringify([row.universeId, row.assetId, row.saleAt, row.saleLocation, row.holdStatus, row.price, row.revenue]);
    const ordinal = occurrences.get(signature) ?? 0;
    occurrences.set(signature, ordinal + 1);
    const fingerprint = row.externalTransactionId ? `id:${await digest(row.externalTransactionId)}` : await digest(`${signature}#${ordinal}`);
    sales.push({ ...row, importBatchId: batchId, saleFingerprint: fingerprint.padEnd(64, "0").slice(0, 64) });
  }
  const dates = sales.map((sale) => sale.saleAt.slice(0, 10)).sort();
  return { rows: sales, skipped, files: files.map((file) => file.name), warnings, from: dates[0], to: dates.at(-1)! };
}

export default function GroupRevenue() {
  const [data, setData] = useState<RobloxGroupRevenueData>({ groups: [], sales: [], passes: [] });
  const [selectedId, setSelectedId] = useState("");
  const [profile, setProfile] = useState<string | null>(null);
  const [erlcxSales, setErlcxSales] = useState<SalesSnapshot | null>(null);
  const [groupId, setGroupId] = useState("");
  const [name, setName] = useState("");
  const [experiences, setExperiences] = useState("");
  const [preview, setPreview] = useState<ParsedPreview | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [apiRefresh, setApiRefresh] = useState({ state: "idle", message: "" });
  const [mappingNames, setMappingNames] = useState<Record<string, string>>({});
  const fileInput = useRef<HTMLInputElement>(null);
  const lastApiState = useRef("idle");

  useEffect(() => {
    if (!desktopAvailable) return;
    let live = true;
    Promise.all([robloxGroupRevenue(), statisticsProfile(), statisticsSales()])
      .then(([next, userId, sales]) => {
        if (!live) return;
        setData(next);
        setProfile(userId);
        setErlcxSales(sales);
        const first = next.groups[0];
        if (first) {
          setSelectedId(first.groupId);
          setGroupId(first.groupId);
          setName(first.name);
          setExperiences(first.experienceIds.join(", "));
        }
      })
      .catch((reason) => live && setError(errorMessage(reason)));
    return () => { live = false; };
  }, []);

  useEffect(() => {
    if (!desktopAvailable) return;
    let live = true;
    const poll = () => void groupRevenueBrowserStatus().then(async (status) => {
      if (!live) return;
      setApiRefresh(status);
      if (lastApiState.current === "running" && status.state === "complete") {
        const next = await robloxGroupRevenue();
        if (live) {
          setData(next);
          const refreshed = next.groups.find((group) => group.groupId === selectedId);
          if (refreshed) setExperiences(refreshed.experienceIds.join(", "));
        }
      }
      lastApiState.current = status.state;
    }).catch(() => {});
    poll();
    const timer = window.setInterval(poll, 1200);
    return () => { live = false; window.clearInterval(timer); };
  }, [selectedId]);

  const selected = data.groups.find((group) => group.groupId === selectedId) ?? null;
  const allGroupSales = useMemo(
    () => data.sales.filter((sale) => sale.groupId === selectedId),
    [data.sales, data.groups, selectedId],
  );
  const selectedPassIds = new Set(selected?.selectedPassIds ?? []);
  const groupSales = allGroupSales.filter((sale) => selectedPassIds.has(sale.assetId));
  const latestImportAt = allGroupSales
    .filter((sale) => sale.importedAt)
    .map((sale) => sale.importedAt!)
    .sort()
    .at(-1) ?? null;
  const assetIds = [...new Set([...allGroupSales.map((sale) => sale.assetId), ...data.passes.filter((pass) => pass.groupId === selectedId).map((pass) => pass.assetId)])].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  const from = groupSales.map((sale) => sale.saleAt.slice(0, 10)).sort()[0] ?? null;
  const to = groupSales.map((sale) => sale.saleAt.slice(0, 10)).sort().at(-1) ?? null;
  const totals = {
    count: groupSales.filter((sale) => sale.holdStatus !== "Cancelled").length,
    released: groupSales.filter((sale) => sale.holdStatus === "Released").reduce((sum, sale) => sum + sale.revenue, 0),
    held: groupSales.filter((sale) => sale.holdStatus === "Held").reduce((sum, sale) => sum + sale.revenue, 0),
    releasedPrice: groupSales.filter((sale) => sale.holdStatus === "Released").reduce((sum, sale) => sum + sale.price, 0),
    heldPrice: groupSales.filter((sale) => sale.holdStatus === "Held").reduce((sum, sale) => sum + sale.price, 0),
    cancelled: groupSales.filter((sale) => sale.holdStatus === "Cancelled").length,
  };
  const mappedNames = selected?.passMappings ?? {};
  const unmappedIds = assetIds.filter((assetId) => !mappedNames[assetId]);
  const catalogPasses = new Map(data.passes.filter((pass) => pass.groupId === selectedId).map((pass) => [pass.assetId, pass]));

  async function refreshData() {
    const next = await robloxGroupRevenue();
    setData(next);
    return next;
  }

  function chooseGroup(id: string) {
    setSelectedId(id);
    const group = data.groups.find((entry) => entry.groupId === id);
    setGroupId(group?.groupId ?? "");
    setName(group?.name ?? "");
    setExperiences(group?.experienceIds.join(", ") ?? "");
    setPreview(null);
    setMappingNames({});
  }

  async function saveGroup(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const ids = [...new Set(experiences.split(/[\s,;]+/).map((value) => value.trim()).filter(Boolean))];
      if (!idPattern.test(groupId.trim()) || ids.some((id) => !idPattern.test(id))) throw new Error("Enter a valid numeric group ID and numeric experience IDs.");
      const group: RobloxRevenueGroup = {
        groupId: groupId.trim(), name: name.trim(), experienceIds: ids,
        passMappings: selected?.groupId === groupId.trim() ? selected.passMappings : {},
        selectedPassIds: selected?.groupId === groupId.trim() ? selected.selectedPassIds : [], updatedAt: null,
      };
      const next = await saveRobloxRevenueGroup(group);
      setData(next);
      setSelectedId(group.groupId);
      setNotice("Group settings saved.");
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function prepareImport(files: FileList | null) {
    setPreview(null);
    setError("");
    setNotice("");
    if (!files?.length || !selected) return;
    setBusy(true);
    try { setPreview(await readCsvFiles(Array.from(files), selected)); }
    catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); if (fileInput.current) fileInput.current.value = ""; }
  }

  async function commitImport() {
    if (!selected || !preview) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await importRobloxGroupSales(selected, preview.rows[0].importBatchId, preview.rows);
      await refreshData();
      setPreview(null);
      setNotice(`Imported ${number(result.inserted)} new Game Pass sales; ${number(result.duplicates)} already existed.`);
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function refreshRobloxApi() {
    if (!selected) return;
    setError("");
    setNotice("");
    try { await startGroupRevenueBrowser(selected.groupId); }
    catch (reason) { setError(errorMessage(reason)); }
  }

  async function saveMappings() {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      const passMappings = { ...selected.passMappings };
      for (const assetId of assetIds) {
        const value = mappingNames[assetId]?.trim();
        if (value) passMappings[assetId] = value;
      }
      const next = await saveRobloxRevenueGroup({ ...selected, passMappings });
      setData(next);
      setMappingNames({});
      setNotice("Game Pass labels saved.");
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function togglePass(assetId: string, checked: boolean) {
    if (!selected) return;
    const selectedPassIds = checked
      ? [...new Set([...selected.selectedPassIds, assetId])]
      : selected.selectedPassIds.filter((id) => id !== assetId);
    setBusy(true);
    setError("");
    try { setData(await saveRobloxRevenueGroup({ ...selected, selectedPassIds })); }
    catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  async function deleteGroup() {
    if (!selected || !window.confirm(`Remove ${selected.name}, its mappings and all imported sales from this workspace?`)) return;
    setBusy(true);
    setError("");
    try {
      const next = await removeRobloxGroupRevenue(selected.groupId);
      setData(next);
      const first = next.groups[0];
      setSelectedId(first?.groupId ?? "");
      setGroupId(first?.groupId ?? "");
      setName(first?.name ?? "");
      setExperiences(first?.experienceIds.join(", ") ?? "");
      setNotice("Group revenue data removed from this workspace.");
    } catch (reason) { setError(errorMessage(reason)); }
    finally { setBusy(false); }
  }

  const comparison = useMemo(() => {
    if (!erlcxSales || !from || !to) return null;
    const days = erlcxSales.data.series.filter((point) => point.date >= from && point.date <= to);
    const daySet = new Set(days.map((point) => point.date));
    const matchingRows = groupSales.filter((sale) => daySet.has(sale.saleAt.slice(0, 10)) && sale.holdStatus !== "Cancelled");
    return {
      days: days.length,
      groupSales: matchingRows.length,
      groupRevenue: matchingRows.reduce((sum, sale) => sum + sale.revenue, 0),
      erlcxSales: days.reduce((sum, point) => sum + point.sales, 0),
      erlcxRevenue: days.reduce((sum, point) => sum + point.netRobux, 0),
    };
  }, [erlcxSales, groupSales, from, to]);

  return (
    <div className="group-revenue-page">
      <header className="group-revenue-header">
        <div><p className="eyebrow">Roblox analytics</p><h1>Group Revenue</h1><p>Import Roblox sales exports and compare aggregate figures with ERLCX observations.</p></div>
        {data.groups.length > 0 && <label className="group-revenue-selector">Selected group<select value={selectedId} onChange={(event) => chooseGroup(event.target.value)}>{data.groups.map((group) => <option key={group.groupId} value={group.groupId}>{group.name} · {group.groupId}</option>)}</select></label>}
      </header>

      {error && <p className="error" role="alert">{error}</p>}
      {notice && <p className="group-revenue-notice" role="status">{notice}</p>}
      {!desktopAvailable && <p className="group-revenue-note">Group Revenue storage is available in the Apex Studio desktop app.</p>}

      <section className="content-section group-revenue-card">
        <div><h2>Group and experiences</h2><p className="field-hint">Group ID scopes Roblox refreshes. Experience IDs are optional before the first refresh; Roblox can discover the group's experiences. Enter IDs manually for CSV import if needed.</p></div>
        <form className="group-revenue-form" onSubmit={(event) => void saveGroup(event)}>
          <label>Group name<input value={name} onChange={(event) => setName(event.target.value)} required maxLength={120} placeholder="Apex Designs" /></label>
          <label>Group ID<input value={groupId} onChange={(event) => setGroupId(event.target.value)} required inputMode="numeric" placeholder="12345678" /></label>
          <label className="group-revenue-wide">Experience IDs<input value={experiences} onChange={(event) => setExperiences(event.target.value)} placeholder="Discovered when you refresh Roblox" /><small>Optional · separate multiple IDs with commas or spaces.</small></label>
          <div className="group-revenue-actions"><button className="primary" type="submit" disabled={busy}>{busy ? "Saving…" : selected ? "Save group settings" : "Add group"}</button>{selected && <button className="secondary" type="button" onClick={() => chooseGroup("")}>New group</button>}</div>
        </form>
      </section>

      {selected && <>
        <section className="content-section group-revenue-card">
          <div className="group-revenue-section-heading"><div><h2>Roblox group data</h2><p className="field-hint">Refresh the experimental Roblox catalog and transaction feed in the connected browser, or import CSV files for backfill.</p></div><div className="group-revenue-actions"><button className="primary" type="button" disabled={busy || apiRefresh.state === "running"} onClick={() => void refreshRobloxApi()}>{apiRefresh.state === "running" ? "Refreshing…" : "Refresh from Roblox"}</button><button className="secondary" type="button" disabled={busy} onClick={() => fileInput.current?.click()}>{busy ? "Reading…" : "Choose CSV files"}</button></div></div>
          {apiRefresh.state !== "idle" && <p className={apiRefresh.state === "failed" ? "error" : "group-revenue-note"} role={apiRefresh.state === "failed" ? "alert" : "status"}>{apiRefresh.message}</p>}
          {selected.apiUpdatedAt && <p className="group-revenue-note">Last successful Roblox API refresh: {new Date(selected.apiUpdatedAt).toLocaleString()} · sale coverage: {selected.apiCoverageFrom && selected.apiCoverageTo ? `${selected.apiCoverageFrom} – ${selected.apiCoverageTo}` : "not established"}. {selected.apiStatus === "partial" ? `${number(selected.apiSummary?.unsupportedTransactions ?? 0)} transactions require CSV review.` : "This experimental feed may be incomplete."}</p>}
          <input ref={fileInput} className="visually-hidden" type="file" accept=".csv,text/csv" multiple onChange={(event) => void prepareImport(event.target.files)} />
          <p className="group-revenue-privacy">Apex keeps transaction fields needed for the report in this workspace. Buyer User ID, the raw CSV and the original rows are not saved.</p>
          {preview && <div className="group-revenue-preview"><h3>Import preview</h3><p>{preview.files.join(", ")} · {preview.from} to {preview.to}</p><p>{number(preview.rows.length)} Game Pass rows for {selected.experienceIds.length} configured experience(s); {number(preview.skipped)} other or out-of-scope rows skipped.</p>{preview.warnings.map((warning) => <p className="group-revenue-note" key={warning}>{warning}</p>)}<p>{preview.rows.some((row) => row.externalTransactionId) ? "Roblox transaction IDs will be used to prevent repeat imports. Buyer User IDs and raw rows are discarded." : "This export has no transaction IDs. Apex matches canonical sale fields and flags uncertain overlaps."}</p><button className="primary" type="button" disabled={busy} onClick={() => void commitImport()}>Import {number(preview.rows.length)} rows</button><button className="secondary" type="button" onClick={() => setPreview(null)}>Cancel</button></div>}
        </section>

        <section className="group-revenue-cards" aria-label="Roblox group revenue summary">
          <article><span>Selected Game Pass sales</span><strong>{number(totals.count)}</strong><small>{selected?.selectedPassIds.length ? (from && to ? `${from} – ${to}` : "No sales for selected passes") : "Select passes to include sales"}</small></article>
          <article><span>Released Revenue</span><strong>{robux(totals.released)}</strong><small>Buyer Price: {robux(totals.releasedPrice)}</small></article>
          <article><span>Held Revenue</span><strong>{robux(totals.held)}</strong><small>Buyer Price: {robux(totals.heldPrice)}</small></article>
          <article><span>Cancelled rows</span><strong>{number(totals.cancelled)}</strong><small>Excluded from revenue totals</small></article>
        </section>
        {selected.apiSummary && Object.entries(selected.apiSummary).some(([key, value]) => key !== "returnedTransactions" && key !== "importedTransactions" && key !== "unsupportedTransactions" && key !== "experiences" && key !== "gamePasses" && Number.isFinite(value)) && <section className="content-section group-revenue-card"><div><h2>Roblox group comparison</h2><p className="field-hint">Group-wide Roblox summary for the API period. This comparison is separate from the selected Game Pass totals above.</p></div><div className="group-revenue-cards">{Object.entries(selected.apiSummary).filter(([key, value]) => key !== "returnedTransactions" && key !== "importedTransactions" && key !== "unsupportedTransactions" && key !== "experiences" && key !== "gamePasses" && Number.isFinite(value)).map(([key, value]) => <article key={key}><span>{key.replace(/[._]/g, " ")}</span><strong>{/revenue|robux|amount/i.test(key) ? robux(value) : number(value)}</strong><small>Roblox group API · monthly summary</small></article>)}</div></section>}
        <p className="group-revenue-note">Source: Roblox Creator Hub sales CSV · Last import: {latestImportAt ? new Date(latestImportAt).toLocaleString() : "not imported"} · Range: {from && to ? `${from} – ${to}` : "none"}. Coverage is limited to the files you selected and the configured experience IDs; Apex cannot independently verify that the exports include every eligible sale. Roblox-generated exports are updated about every 48 hours.</p>

        <section className="content-section group-revenue-card">
          <div><h2>Game Pass selection</h2><p className="field-hint">Only selected passes contribute to sales and revenue. New passes start unselected; IDs missing from the catalog remain available for review.</p></div>
          {assetIds.length ? <div className="group-revenue-map-list">{assetIds.map((assetId) => { const pass = catalogPasses.get(assetId); return <label key={assetId}><span><input type="checkbox" checked={selectedPassIds.has(assetId)} disabled={busy} onChange={(event) => void togglePass(assetId, event.target.checked)} /><strong>{mappingNames[assetId] ?? mappedNames[assetId] ?? pass?.name ?? `Unknown Game Pass ${assetId}`}</strong><small>{pass ? `${pass.name} · Experience ${pass.universeId}` : `Not found in catalog · ID ${assetId}`}</small></span><input aria-label={`Label for Game Pass ${assetId}`} value={mappingNames[assetId] ?? mappedNames[assetId] ?? ""} onChange={(event) => setMappingNames((current) => ({ ...current, [assetId]: event.target.value }))} placeholder="Optional label" /></label>; })}</div> : <p>No Game Pass IDs imported or loaded yet.</p>}
          {(unmappedIds.length > 0 || Object.keys(mappingNames).length > 0) && <button className="secondary" type="button" disabled={busy} onClick={() => void saveMappings()}>Save labels</button>}
          {unmappedIds.length === 0 && assetIds.length > 0 && <p className="group-revenue-note">All imported Game Pass IDs have labels.</p>}
        </section>

        <section className="content-section group-revenue-card">
          <div><h2>ERLCX comparison</h2><p className="field-hint">Aggregate sources stay separate. The comparison uses only dates present in both the imported CSV and the ERLCX 30-day series.</p></div>
          {!profile && <p className="group-revenue-note">Configure an ERLCX Roblox profile in Statistics to load its saved aggregate snapshot.</p>}
          {profile && !erlcxSales && <p className="group-revenue-note">No ERLCX creator sales snapshot is saved yet. Refresh it in Statistics first.</p>}
          {comparison && <><p className="field-hint">ERLCX creator profile {profile} · Snapshot {new Date(erlcxSales!.collectedAt).toLocaleString()} · {comparison.days} matching day(s) between {from} and {to}</p><div className="group-revenue-comparison"><div><span>Roblox group CSV</span><strong>{number(comparison.groupSales)} sales</strong><small>{robux(comparison.groupRevenue)} Revenue</small></div><div><span>ERLCX creator dashboard</span><strong>{number(comparison.erlcxSales)} sales</strong><small>{robux(comparison.erlcxRevenue)} netRobux</small></div><div><span>Aggregate difference</span><strong>{number(comparison.groupSales - comparison.erlcxSales)} sales</strong><small>{robux(comparison.groupRevenue - comparison.erlcxRevenue)} reported amount</small></div></div><p className="group-revenue-note">This is an aggregate comparison only. ERLCX currently provides aggregate sales, so Apex cannot identify which Roblox rows may be absent from ERLCX. CSV and ERLCX revenue definitions and account scopes may differ.</p></>}
        </section>

        <section className="content-section group-revenue-card">
          <div className="group-revenue-section-heading"><div><h2>Imported sales</h2><p className="field-hint">Showing selected Game Passes only. Choose passes above to include their sales.</p></div><button className="text-button group-revenue-delete" type="button" disabled={busy} onClick={() => void deleteGroup()}>Remove group data</button></div>
          {groupSales.length ? <div className="group-revenue-table-wrap"><table className="group-revenue-table"><thead><tr><th>Date (UTC)</th><th>Experience ID</th><th>Game Pass</th><th>Sale location</th><th>Status</th><th>Source / match</th><th>Price</th><th>Revenue</th></tr></thead><tbody>{groupSales.slice(0, 500).map((sale) => <tr key={sale.saleFingerprint}><td>{sale.saleAt.replace("T", " ").replace(".000Z", " UTC")}</td><td>{sale.universeId}</td><td>{mappedNames[sale.assetId] ? `${mappedNames[sale.assetId]} · ` : ""}{sale.assetId}</td><td>{sale.saleLocation || "—"}</td><td>{sale.holdStatus}</td><td>{sale.source === "api" ? "Roblox API" : "CSV"}{sale.dedupeConfidence === "ambiguous" ? " · review match" : ""}</td><td>{robux(sale.price)}</td><td>{robux(sale.revenue)}</td></tr>)}</tbody></table>{groupSales.length > 500 && <p className="field-hint">Showing latest 500 of {number(groupSales.length)} sales.</p>}</div> : <p>No imported sales for this group yet.</p>}
        </section>
      </>}
      {!selected && data.groups.length === 0 && <p className="group-revenue-note">Add a group above to start. CSV import remains manual; experimental Roblox APIs are not enabled.</p>}
      {!selected && data.groups.length > 0 && <p className="group-revenue-note">Choose or create a group above.</p>}
    </div>
  );
}
