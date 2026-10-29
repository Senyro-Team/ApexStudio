// Runs in the ERLCX dashboard's page context. Only aggregate fields cross the bridge.
export async function readCreatorStatistics(expectedProfile) {
  if (
    location.origin !== "https://erlcx.com" ||
    location.pathname !== "/creator/dashboard"
  )
    throw Error("Open the ERLCX creator dashboard.");
  const read = async (url) => {
    const response = await fetch(url, {
      credentials: "include",
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) throw Error("Creator dashboard access failed.");
    return response.json();
  };
  const identity = await read("https://api.erlcx.com/users/me");
  if (String(identity?.profile?.sub ?? "") !== expectedProfile)
    throw Error(
      "The signed-in ERLCX account does not match the selected user.",
    );
  const result = await read(
    "https://api.erlcx.com/users/me/monetization/earnings?days=30&limit=1",
  );
  const count = (value) => {
    if (!Number.isSafeInteger(value) || value < 0)
      throw Error("Invalid creator statistics.");
    return value;
  };
  if (
    result.ok !== true ||
    typeof result.currency !== "string" ||
    result.currency.toLowerCase() !== "robux" ||
    !Array.isArray(result.series) ||
    result.series.length > 31
  )
    throw Error("Invalid creator statistics.");
  return {
    sales: count(result.summary?.purchases),
    allTimeSales: count(result.summary?.allTimePurchases),
    netRobux: count(result.summary?.netRobux),
    grossRobux: count(result.summary?.grossRobux),
    series: result.series.map((p) => {
      if (typeof p.date !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(p.date))
        throw Error("Invalid creator statistics.");
      return {
        date: p.date,
        sales: count(p.sales),
        netRobux: count(p.netRobux),
      };
    }),
  };
}
