const seconds = (value) => value * 1000;

function check(signal) {
  if (signal.aborted) throw signal.reason;
}

async function poll(invoke, signal, limit, accept) {
  const end = performance.now() + limit;
  do {
    check(signal);
    const status = await invoke("status");
    if (accept(status)) return status;
    await new Promise((resolve) => setTimeout(resolve, 240));
  } while (performance.now() < end);
  check(signal);
  return null;
}

const settled = (status) => status.phase === "idle" || status.phase === "error";
const ready = (status, nr) => status.phase === "running" && status.overlay
  && status.presentFps > 0 && status.captureFps > 0 && (!nr || status.nrHeight > 0);

function median(items) {
  const sorted = items.filter(Number.isFinite).sort((a, b) => a - b);
  return sorted.length ? sorted[Math.floor(sorted.length / 2)] : null;
}

function requiredFactor(goal, input) {
  return Math.max(2, Math.min(6, Math.ceil(goal / (Math.max(input, 1) * 1.02))));
}

function profiles(base, factor) {
  const multiplier = base.multiplier === 1 ? 1 : factor;
  return [
    { label: "Текущие параметры", settings: { ...base, multiplier } },
    { label: "Качество", settings: { ...base, multiplier, height: 1080, quality: 2, nrResolution: 720 } },
    { label: "Баланс", settings: { ...base, multiplier, height: 1080, quality: 1, nrResolution: 480, nrPasses: 1 } },
    { label: "Производительность", settings: { ...base, multiplier, height: 900, quality: 1, nrResolution: 480, nrPasses: 1, scale: 1.25 } },
    { label: "Минимальная нагрузка", settings: { ...base, multiplier, height: 720, quality: 0, nrResolution: 480, nrPasses: 1, scale: 1.25 } },
  ];
}

function profileKey(s) {
  return [s.height, s.multiplier, s.multiplier === 1 ? 0 : s.quality, s.sr ? s.scale : 0,
    s.nr ? s.nrResolution : 0, s.nr ? s.nrPasses : 0].join("/");
}

function qualityScore(s) {
  return (s.height || 2160) * 4 + (s.multiplier === 1 ? 0 : s.quality * 380)
    + (s.sr ? s.scale * 280 : 0) + (s.nr ? (s.nrResolution || 2160) * .65 + s.nrPasses * 60 : 0);
}

async function measure(invoke, signal, profile, lockedInput) {
  await invoke("test_profile", { settings: { ...profile.settings, input: lockedInput || profile.settings.input } });
  const status = await poll(invoke, signal, seconds(19), (s) => ready(s, profile.settings.nr) || s.phase === "error" || s.nrError);
  if (!status || status.phase === "error" || status.nrError) {
    return { ...profile, error: status?.nrError || status?.message || "Кадры не поступают. Проверьте, что окно открыто и видео или игра продолжаются." };
  }
  if (status.autoInput && !status.cadenceLocked) {
    const cadence = await poll(invoke, signal, seconds(5), (s) => s.cadenceLocked || s.phase === "error" || s.nrError);
    if (cadence && !ready(cadence, profile.settings.nr)) {
      const resumed = await poll(invoke, signal, seconds(14), (s) => ready(s, profile.settings.nr) || s.phase === "error" || s.nrError);
      if (!resumed || !ready(resumed, profile.settings.nr)) return { ...profile, error: resumed?.nrError || resumed?.message || "Захват не возобновился после определения FPS." };
    }
  }
  const samples = [];
  let last = status;
  for (let i = 0; i < 8; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 480));
    check(signal);
    last = await invoke("status");
    if (last.phase === "error" || last.nrError) return { ...profile, error: last.nrError || last.message };
    if (ready(last, profile.settings.nr)) samples.push(last);
  }
  const output = samples.map((sample) => sample.presentFps);
  if (output.length < 6) {
    return { ...profile, error: "Слишком мало кадров для замера. Держите исходное окно открытым и запустите движущуюся сцену." };
  }
  return {
    ...profile,
    fps: median(output),
    low: Math.min(...output),
    capture: median(samples.map((sample) => sample.captureFps)),
    delay: median(samples.map((sample) => sample.latencyMs)),
    hz: last.hz,
    input: last.input,
    cadenceLocked: last.cadenceLocked,
  };
}

export async function tuneWindow({ invoke, source, settings, goal, effects, signal, progress }) {
  const base = { ...settings, sr: effects.sr, hdr: effects.hdr, nr: effects.nr,
    multiplier: effects.vfg ? requiredFactor(goal, settings.input || 30) : 1 };
  const results = [];
  const tried = new Set();
  let factor = base.multiplier;
  let lockedInput = 0;
  let started = false;
  async function tryProfile(profile) {
    check(signal);
    const key = profileKey(profile.settings);
    if (tried.has(key)) return;
    tried.add(key);
    const previous = await invoke("status");
    if (!settled(previous)) {
      await invoke("stop");
      if (!await poll(invoke, signal, seconds(10), settled)) throw Error("Предыдущий тест ещё завершает работу.");
    }
    progress(profile.label, results.length + 1);
    results.push(await measure(invoke, signal, profile, lockedInput));
  }
  const qualifies = (r) => r.fps >= goal * 0.98 && r.low >= goal * 0.90 && (!r.hz || r.hz >= goal - 0.5);
  try {
    check(signal);
    await invoke("begin_tuning", { source });
    started = true;
    check(signal);
    await tryProfile(profiles(base, factor)[2]);
    if (settings.input === 0 && results[0].cadenceLocked
      && [24, 25, 30, 48, 50, 60, 72, 90, 120].includes(results[0].input)) {
      lockedInput = results[0].input;
    }
    if (effects.vfg && results[0].input) factor = requiredFactor(goal, results[0].input);
    for (const profile of profiles(base, factor)) {
      await tryProfile(profile);
    }
    const near = results.filter((result) => result.fps >= goal * 0.66 && (!result.hz || result.hz >= goal - 0.5));
    if (effects.vfg && factor < 6 && near.length && !results.some(qualifies)) {
      const closest = near.sort((a, b) => b.fps - a.fps)[0];
      const nextFactor = Math.min(6, Math.max(factor + 1, Math.ceil(closest.settings.multiplier * goal / closest.fps)));
      await tryProfile({ label: `Повышенный множитель ${nextFactor}×`, settings: { ...closest.settings, multiplier: nextFactor } });
    }
  } finally {
    if (started) await invoke("end_tuning");
  }
  check(signal);
  const measured = results.filter((result) => result.fps !== undefined);
  const reaching = measured.filter(qualifies)
    .sort((a, b) => qualityScore(b.settings) - qualityScore(a.settings) || (a.delay ?? Infinity) - (b.delay ?? Infinity));
  const fastest = measured.sort((a, b) => b.fps - a.fps || b.low - a.low);
  return { best: reaching[0] || fastest[0], reached: reaching.length > 0, results };
}
