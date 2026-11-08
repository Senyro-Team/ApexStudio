import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  desktopAvailable,
  errorMessage,
  importRobloxGroupSales,
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

function parseCsv(source: string): string[][] {
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

function amount(value: string, label: string) {
  const clean = value.replace(/[\s,]/g, "").replace(/^R\$/i, "");
  if (!/^\d+$/.test(clean)) throw new Error(`${label} must be a whole number of Robux.`);
  const parsed = Number(clean);
  if (!Number.isSafeInteger(parsed)) throw new Error(`${label} is outside the supported range.`);
  return parsed;
}

type ParsedPreview = {
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

async function readCsvFiles(files: File[], group: RobloxRevenueGroup): Promise<ParsedPreview> {
  const experiences = new Set(group.experienceIds);
  const rows: Omit<RobloxGroupSale, "saleFingerprint">[] = [];
  let skipped = 0;
  const warnings: string[] = [];
  for (const file of files) {
    if (file.size > 25 * 1024 * 1024) throw new Error(`${file.name} is larger than 25 MB.`);
    const parsed = parseCsv(await file.text());
    const columns = new Map(parsed[0].map((header, index) => [normalizeHeader(header), index]));
    const required = ["saledateandtime", "salelocation", "universeid", "assetid", "assettype", "holdstatus", "revenue", "price"];
    const missing = required.filter((name) => !columns.has(name));
    if (missing.length) throw new Error(`${file.name} is missing required Roblox CSV columns: ${missing.join(", ")}.`);
    const recognized = new Set([...required, "buyeruserid", "universe", "assetname"]);
    const unexpected = [...columns.keys()].filter((header) => !recognized.has(header));
    if (unexpected.length) warnings.push(`${file.name}: ignored unrecognized column(s): ${unexpected.join(", ")}.`);
    for (let line = 1; line < parsed.length; line++) {
      const row = parsed[line];
      const type = normalizeHeader(readField(row, columns, "assettype"));
      const universeId = readField(row, columns, "universeid");
      if (type !== "gamepass" || !experiences.has(universeId)) {
        skipped++;
        continue;
      }
      const assetId = readField(row, columns, "assetid");
      if (!idPattern.test(universeId) || !idPattern.test(assetId)) throw new Error(`${file.name}, row ${line + 1}: invalid experience or Game Pass asset ID.`);
      const holdValue = normalizeHeader(readField(row, columns, "holdstatus"));
      const holdStatus = holdValue === "released" ? "Released" : holdValue === "held" ? "Held" : holdValue === "cancelled" || holdValue === "canceled" ? "Cancelled" : null;
      if (!holdStatus) throw new Error(`${file.name}, row ${line + 1}: unknown hold status.`);
      rows.push({
        universeId,
        assetId,
        saleAt: parseDate(readField(row, columns, "saledateandtime")),
        saleLocation: readField(row, columns, "salelocation"),
        holdStatus,
        revenue: amount(readField(row, columns, "revenue"), "Revenue"),
        price: amount(readField(row, columns, "price"), "Price"),
        importBatchId: "",
      });
    }
  }
  if (!rows.length) throw new Error("No Game Pass sales for the configured experiences were found in these CSV files.");

  // Roblox's export has no transaction ID. This stable signature intentionally omits Buyer User ID.
  // Identical rows are retained by adding their ordinal within the import, so repeat imports stay idempotent.
  const occurrences = new Map<string, number>();
  const batchId = crypto.randomUUID();
  const sales: RobloxGroupSale[] = [];
  for (const row of rows) {
    const signature = JSON.stringify([row.universeId, row.assetId, row.saleAt, row.saleLocation, row.holdStatus, row.price, row.revenue]);
    const ordinal = occurrences.get(signature) ?? 0;
    occurrences.set(signature, ordinal + 1);
    sales.push({ ...row, importBatchId: batchId, saleFingerprint: await digest(`${signature}#${ordinal}`) });
  }
  const dates = sales.map((sale) => sale.saleAt.slice(0, 10)).sort();
  return { rows: sales, skipped, files: files.map((file) => file.name), warnings, from: dates[0], to: dates.at(-1)! };
}

export default function GroupRevenue() {
  const [data, setData] = useState<RobloxGroupRevenueData>({ groups: [], sales: [] });
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
  const [mappingNames, setMappingNames] = useState<Record<string, string>>({});
  const fileInput = useRef<HTMLInputElement>(null);

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

  const selected = data.groups.find((group) => group.groupId === selectedId) ?? null;
  const groupSales = useMemo(
    () => data.sales.filter((sale) => sale.groupId === selectedId),
    [data.sales, data.groups, selectedId],
  );
  const latestImportAt = data.sales
    .filter((sale) => sale.groupId === selectedId && sale.importedAt)
    .map((sale) => sale.importedAt!)
    .sort()
    .at(-1) ?? null;
  const assetIds = [...new Set(groupSales.map((sale) => sale.assetId))].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  const from = groupSales.map((sale) => sale.saleAt.slice(0, 10)).sort()[0] ?? null;
  const to = groupSales.map((sale) => sale.saleAt.slice(0, 10)).sort().at(-1) ?? null;
  const totals = {
    count: groupSales.length,
    released: groupSales.filter((sale) => sale.holdStatus === "Released").reduce((sum, sale) => sum + sale.revenue, 0),
    held: groupSales.filter((sale) => sale.holdStatus === "Held").reduce((sum, sale) => sum + sale.revenue, 0),
    cancelled: groupSales.filter((sale) => sale.holdStatus === "Cancelled").length,
  };
  const mappedNames = selected?.passMappings ?? {};
  const unmappedIds = assetIds.filter((assetId) => !mappedNames[assetId]);

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
      if (!idPattern.test(groupId.trim()) || !ids.length || ids.some((id) => !idPattern.test(id))) throw new Error("Enter numeric Roblox IDs for the group and at least one experience.");
      const group: RobloxRevenueGroup = {
        groupId: groupId.trim(), name: name.trim(), experienceIds: ids,
        passMappings: selected?.groupId === groupId.trim() ? selected.passMappings : {}, updatedAt: null,
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
        <div><h2>Group and experiences</h2><p className="field-hint">Set the IDs that scope imports. Experience IDs are matched against the export's Universe ID.</p></div>
        <form className="group-revenue-form" onSubmit={(event) => void saveGroup(event)}>
          <label>Group name<input value={name} onChange={(event) => setName(event.target.value)} required maxLength={120} placeholder="Apex Designs" /></label>
          <label>Group ID<input value={groupId} onChange={(event) => setGroupId(event.target.value)} required inputMode="numeric" placeholder="12345678" /></label>
          <label className="group-revenue-wide">Experience IDs<input value={experiences} onChange={(event) => setExperiences(event.target.value)} required placeholder="123456789, 987654321" /><small>Separate multiple IDs with commas or spaces.</small></label>
          <div className="group-revenue-actions"><button className="primary" type="submit" disabled={busy}>{busy ? "Saving…" : selected ? "Save group settings" : "Add group"}</button>{selected && <button className="secondary" type="button" onClick={() => chooseGroup("")}>New group</button>}</div>
        </form>
      </section>

      {selected && <>
        <section className="content-section group-revenue-card">
          <div className="group-revenue-section-heading"><div><h2>Import Roblox sales</h2><p className="field-hint">Roblox CSV exports are updated about every 48 hours. Extract the CSV from Roblox's downloaded ZIP first.</p></div><button className="secondary" type="button" disabled={busy} onClick={() => fileInput.current?.click()}>{busy ? "Reading…" : "Choose CSV files"}</button></div>
          <input ref={fileInput} className="visually-hidden" type="file" accept=".csv,text/csv" multiple onChange={(event) => void prepareImport(event.target.files)} />
          <p className="group-revenue-privacy">Apex keeps transaction fields needed for the report in this workspace. Buyer User ID, the raw CSV and the original rows are not saved.</p>
          {preview && <div className="group-revenue-preview"><h3>Import preview</h3><p>{preview.files.join(", ")} · {preview.from} to {preview.to}</p><p>{number(preview.rows.length)} Game Pass rows for {selected.experienceIds.length} configured experience(s); {number(preview.skipped)} other or out-of-scope rows skipped.</p>{preview.warnings.map((warning) => <p className="group-revenue-note" key={warning}>{warning}</p>)}<p>No Roblox transaction ID is included in this export. Apex uses the available date, experience, pass, location, status, price and revenue fields for repeat-import deduplication. Identical transactions cannot always be distinguished perfectly.</p><button className="primary" type="button" disabled={busy} onClick={() => void commitImport()}>Import {number(preview.rows.length)} rows</button><button className="secondary" type="button" onClick={() => setPreview(null)}>Cancel</button></div>}
        </section>

        <section className="group-revenue-cards" aria-label="Roblox group revenue summary">
          <article><span>Game Pass sales</span><strong>{number(totals.count)}</strong><small>{from && to ? `${from} – ${to}` : "No imported sales"}</small></article>
          <article><span>Released revenue</span><strong>{robux(totals.released)}</strong><small>Roblox CSV Revenue</small></article>
          <article><span>Held revenue</span><strong>{robux(totals.held)}</strong><small>Separate from released Robux</small></article>
          <article><span>Cancelled rows</span><strong>{number(totals.cancelled)}</strong><small>Excluded from sales comparison</small></article>
        </section>
        <p className="group-revenue-note">Source: Roblox Creator Hub sales CSV · Last import: {latestImportAt ? new Date(latestImportAt).toLocaleString() : "not imported"} · Range: {from && to ? `${from} – ${to}` : "none"}. Coverage is limited to the files you selected and the configured experience IDs; Apex cannot independently verify that the exports include every eligible sale. Roblox-generated exports are updated about every 48 hours.</p>

        <section className="content-section group-revenue-card">
          <div><h2>Game Pass mapping</h2><p className="field-hint">IDs determine matching. Names are labels only; unknown IDs remain visible.</p></div>
          {assetIds.length ? <div className="group-revenue-map-list">{assetIds.map((assetId) => <label key={assetId}><span>Asset ID <strong>{assetId}</strong></span><input value={mappingNames[assetId] ?? mappedNames[assetId] ?? ""} onChange={(event) => setMappingNames((current) => ({ ...current, [assetId]: event.target.value }))} placeholder="Game Pass display name" /></label>)}</div> : <p>No Game Pass IDs imported yet.</p>}
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
          <div className="group-revenue-section-heading"><div><h2>Imported sales</h2><p className="field-hint">Only configured experiences and Game Pass rows are shown.</p></div><button className="text-button group-revenue-delete" type="button" disabled={busy} onClick={() => void deleteGroup()}>Remove group data</button></div>
          {groupSales.length ? <div className="group-revenue-table-wrap"><table className="group-revenue-table"><thead><tr><th>Date (UTC)</th><th>Experience ID</th><th>Game Pass</th><th>Sale location</th><th>Status</th><th>Price</th><th>Revenue</th></tr></thead><tbody>{groupSales.slice(0, 500).map((sale) => <tr key={sale.saleFingerprint}><td>{sale.saleAt.replace("T", " ").replace(".000Z", " UTC")}</td><td>{sale.universeId}</td><td>{mappedNames[sale.assetId] ? `${mappedNames[sale.assetId]} · ` : ""}{sale.assetId}</td><td>{sale.saleLocation || "—"}</td><td>{sale.holdStatus}</td><td>{robux(sale.price)}</td><td>{robux(sale.revenue)}</td></tr>)}</tbody></table>{groupSales.length > 500 && <p className="field-hint">Showing latest 500 of {number(groupSales.length)} sales.</p>}</div> : <p>No imported sales for this group yet.</p>}
        </section>
      </>}
      {!selected && data.groups.length === 0 && <p className="group-revenue-note">Add a group above to start. CSV import remains manual; experimental Roblox APIs are not enabled.</p>}
      {!selected && data.groups.length > 0 && <p className="group-revenue-note">Choose or create a group above.</p>}
    </div>
  );
}
