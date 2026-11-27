import React, { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Activity, ArrowRight, ChartNoAxesCombined, Clock3, Expand, FileText, Film, Keyboard,
  GripHorizontal, Layers3, Maximize, Minus, Monitor, Moon, Play, Radio, RefreshCw, ScanLine, Settings2, Sparkles,
  PanelTop, Scan, Square, Sun, Volume2, VolumeX, X,
} from "lucide-react";
import { Button } from "./components/ui/button";
import { Card } from "./components/ui/card";
import { Dialog, DialogContent, DialogTrigger } from "./components/ui/dialog";
import { Select, SelectGroup, SelectItem } from "./components/ui/select";
import { Switch } from "./components/ui/switch";
import { reducedMotion, useGlass } from "./lib/glass";
import { tuneWindow } from "./lib/tuning";
import brand from "./assets/panevanta.png";
import prism from "./assets/panevanta-cover.png";
import "@fontsource-variable/ibm-plex-sans/wght.css";
import "./styles.css";

const defaults = {
  settings: { input: 0, multiplier: 4, height: 1080, quality: 1, sr: true, scale: 1.5, hdr: true, peak: 0, nr: true, nrDetail: true, nrResolution: 480, nrStyle: 0, nrPasses: 1, nrTone: 100, nrStructure: 100, nrSkin: 100, nrAutoMask: true, nrUiCorrection: true, streamer: false },
  sourceModeScreen: true,
  hotkey: "Shift+Alt+S",
  theme: "system",
  mainOnTop: true,
  metrics: true,
  sound: true,
  motion: true,
};
const stateLabels = { idle: "Ожидание", starting: "Подготовка", stopping: "Остановка", running: "Обработка", error: "Ошибка" };
const fps = [[0, "Авто FPS"], ...[24, 25, 30, 48, 50, 60, 72, 90, 120].map((n) => [n, `${n} FPS`])];
const factors = [...[2, 3, 4, 5, 6].map((n) => [n, `${n}×`]), [0, "Авто по герцовке"]];
const qualities = [[0, "Низкое"], [1, "Среднее"], [2, "Высокое"]];
const heights = [[0, "Исходное"], [1080, "До 1080p"], [900, "До 900p"], [720, "До 720p"]];
const scales = [1.01, 1.25, 1.5, 2].map((n) => [n, `${n}×`]);
const peaks = [[0, "Авто по дисплею"], ...[250, 350, 500, 750, 1000].map((n) => [n, `${n} нит`])];
const nrResolutions = [[480, "480p"], [720, "720p"], [900, "900p"], [1080, "1080p"], [1440, "1440p"], [2160, "2160p · 4K"], [0, "Разрешение монитора"]];
const nrStyles = [[0, "A"], [1, "B"], [2, "C"]];
const nrPasses = [[1, "1"], [2, "2"], [3, "3"]];
const themes = [["system", "Как в Windows"], ["dark", "Тёмная"], ["light", "Светлая"]];
const desktop = "__TAURI_INTERNALS__" in window;
const appWindow = desktop ? getCurrentWindow() : null;
const windowsTheme = window.matchMedia("(prefers-color-scheme: dark)");
const decimal = new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 1 });
const version = __PANE_VERSION__;

function displayNumber(value) { return Number.isFinite(value) ? decimal.format(value) : "—"; }
function sourceKey(source) { return `${source.id.screen ? "screen" : "window"}:${source.id.handle}`; }

function useMedia(query) {
  const [value, setValue] = useState(query.matches);
  useEffect(() => {
    const change = (event) => setValue(event.matches);
    query.addEventListener("change", change);
    return () => query.removeEventListener("change", change);
  }, [query]);
  return value;
}

function IconButton({ label, icon: Icon, ...props }) {
  return <Button size="icon" type="button" title={label} aria-label={label} {...props}><Icon size={17} strokeWidth={1.75} /></Button>;
}

function Field({ id, label, value, onChange, options, disabled = false }) {
  return (
    <div className="pv-field">
      <label htmlFor={id}>{label}</label>
      <Select id={id} value={value} onValueChange={onChange} disabled={disabled}>
        {options.map(([key, caption]) => <SelectItem key={key} value={key}>{caption}</SelectItem>)}
      </Select>
    </div>
  );
}

function NrStyle({ value, onChange, disabled }) {
  return (
    <fieldset className="pv-nr-style" disabled={disabled}>
      <legend>Стиль NR</legend>
      <div>{nrStyles.map(([key, label]) => (
        <label key={key}>
          <input type="radio" name="nr-style" value={key} checked={value === key} onChange={() => onChange(key)} />
          <span>{label}</span>
        </label>
      ))}</div>
    </fieldset>
  );
}

function NrLevel({ id, label, value, onCommit, disabled }) {
  const [draft, setDraft] = useState(null);
  const level = draft ?? value;
  function commit(event) {
    const next = event.currentTarget.valueAsNumber;
    setDraft(null);
    if (next !== value) onCommit(next);
  }
  return (
    <div className="pv-nr-level">
      <label htmlFor={id}>{label}</label>
      <input id={id} type="range" min="0" max="100" step="1" value={level} disabled={disabled}
        onChange={(event) => setDraft(event.currentTarget.valueAsNumber)}
        onPointerUp={commit} onPointerCancel={() => setDraft(null)}
        onKeyUp={commit} onBlur={commit} />
      <output htmlFor={id}>{level}%</output>
    </div>
  );
}

function Effect({ icon: Icon, title, description, enabled, onToggle, disabled, children }) {
  return (
    <section className="pv-effect" data-enabled={enabled}>
      <div className="pv-effect-head">
        <span className="pv-icon-frame"><Icon size={20} strokeWidth={1.65} aria-hidden="true" /></span>
        <div className="min-w-0 flex-1"><h3>{title}</h3><p>{description}</p></div>
        <Switch aria-label={title} checked={enabled} onCheckedChange={onToggle} disabled={disabled} />
      </div>
      <div className="pv-effect-fields">{children}</div>
    </section>
  );
}

function Metric({ label, value, unit, icon: Icon }) {
  return (
    <div className="pv-metric">
      <dt><Icon size={15} strokeWidth={1.7} aria-hidden="true" />{label}</dt>
      <dd>{value}{value !== "—" && <small>{unit}</small>}</dd>
    </div>
  );
}

function InterfaceSettings({ prefs, ready, onTheme, onMotion, onSound, onTop, onOpen }) {
  return (
    <Dialog onOpenChange={(open) => { if (open) onOpen(); }}>
      <DialogTrigger asChild><IconButton icon={Settings2} label="Настройки интерфейса" disabled={!ready} /></DialogTrigger>
      <DialogContent title="Настройки интерфейса" description="Оформление и отклик элементов Panevanta.">
        <Field id="theme" label="Цветовая тема" options={themes} value={prefs.theme} onChange={onTheme} />
        <div className="pv-pref-row">
          <div><b>Звуки действий</b><small>Выбор, старт, остановка и ошибки.</small></div>
          <Switch aria-label="Звуки действий" checked={prefs.sound} onCheckedChange={onSound} />
        </div>
        <div className="pv-pref-row">
          <div><b>Визуальные эффекты</b><small>Стекло, подсветка курсора и анимации.</small></div>
          <Switch aria-label="Визуальные эффекты" checked={prefs.motion} onCheckedChange={onMotion} />
        </div>
        <div className="pv-pref-row">
          <div><b>Основное окно поверх остальных</b><small>Компактная панель всегда поверх окон.</small></div>
          <Switch aria-label="Основное окно поверх остальных" checked={prefs.mainOnTop} onCheckedChange={onTop} />
        </div>
      </DialogContent>
    </Dialog>
  );
}

const tuningEffects = [["vfg", "Генерация кадров"], ["sr", "RTX SR"], ["hdr", "RTX HDR"], ["nr", "DLSS 5"]];
const targets = [24, 30, 48, 60, 72, 90, 100, 120, 144, 165, 180, 240].map((value) => [value, `${value} FPS`]);

function TuneDialog({ ready, source, settings, beforeStart, onApply }) {
  const [open, setOpen] = useState(false);
  const [effects, setEffects] = useState({});
  const [goal, setGoal] = useState(120);
  const [working, setWorking] = useState(false);
  const [step, setStep] = useState("");
  const [result, setResult] = useState(null);
  const [error, setError] = useState("");
  const cancellation = useRef(null);
  const device = useRef(null);

  function changeOpen(next) {
    if (!next && cancellation.current) {
      cancellation.current.abort();
      return;
    }
    if (next) {
      device.current = source.id;
      setEffects({ vfg: settings.multiplier !== 1, sr: settings.sr, hdr: settings.hdr, nr: settings.nr });
      setResult(null);
      setError("");
    }
    setOpen(next);
  }

  async function start() {
    if (cancellation.current) return;
    const task = new AbortController();
    cancellation.current = task;
    setWorking(true);
    setError("");
    setResult(null);
    setStep("Проверяю выбранное окно…");
    try {
      await beforeStart();
      const next = await tuneWindow({
        invoke, source: device.current, settings, goal, effects, signal: task.signal,
        progress: (label, index) => setStep(`Замер ${index} · ${label}`),
      });
      setResult(next);
    } catch (reason) {
      if (task.signal.aborted) {
        setStep("Автоподбор отменён.");
      } else {
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    } finally {
      cancellation.current = null;
      setWorking(false);
    }
  }

  const best = result?.best;
  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <Button variant="glass" type="button" disabled={!ready || !source || source.id.screen} className="pv-tune-button">
          <ScanLine size={16} /> Автоподбор
        </Button>
      </DialogTrigger>
      <DialogContent title="Автоподбор настроек" description="Запустите видео или игру в выбранном окне. Во время замеров держите источник открытым и оставьте сцену в движении.">
        <div className="pv-tune-controls">
          <Field id="tune-goal" label="Целевой вывод" options={targets} value={goal}
            onChange={(value) => { setGoal(Number(value)); setResult(null); }} disabled={working} />
          <div className="pv-tune-effects">
            {tuningEffects.map(([key, name]) => <label key={key}>
              <input type="checkbox" checked={Boolean(effects[key])} disabled={working}
                onChange={(event) => { setEffects({ ...effects, [key]: event.target.checked }); setResult(null); }} />
              {name}
            </label>)}
          </div>
          <div className="pv-tune-status" aria-live="polite">
            {working && <p>{step}</p>}
            {!!error && <p data-error>{error}</p>}
            {!!result && !best && <p data-error>{result.results.find((trial) => trial.error)?.error || "Замер не дал устойчивых кадров."}</p>}
            {best && <>
              <h3>{result.reached ? "Цель достигнута" : "Цель на тесте не достигнута"}</h3>
              <p>{best.label} · <b>{displayNumber(best.fps)} FPS</b> · минимум {displayNumber(best.low)} FPS · {displayNumber(best.delay)} мс</p>
              <p>{best.settings.height ? `До ${best.settings.height}p` : "Исходное разрешение"}{best.settings.multiplier > 1 ? ` · VFG ${best.settings.multiplier}×` : ""}{best.settings.nr ? ` · NR ${best.settings.nrResolution ? `${best.settings.nrResolution}p` : "дисплей"}` : ""}</p>
              {best.hz > 0 && best.hz < goal && <p>Частота дисплея в этом сеансе — {displayNumber(best.hz)} Гц.</p>}
              {!result.reached && <p>Если замеру мешала фоновая нагрузка, закройте лишние приложения, переключите профиль питания на производительный и повторите тест.</p>}
              <details>
                <summary>Все замеры</summary>
                <ul>{result.results.map((trial, index) => <li key={index}>{trial.label} · {trial.error || `${displayNumber(trial.fps)} FPS · ${displayNumber(trial.delay)} мс`}</li>)}</ul>
              </details>
            </>}
            {!working && !result && !error && step === "Автоподбор отменён." && <p>{step}</p>}
          </div>
          <div className="pv-tune-actions">
            {working
              ? <Button variant="glass" type="button" onClick={() => cancellation.current?.abort()}>Отменить</Button>
              : <Button variant="glass" type="button" disabled={!Object.values(effects).some(Boolean)} onClick={start}>{result ? "Повторить" : "Начать замер"}</Button>}
            {!!best && !working && <Button type="button" onClick={() => { onApply(best.settings); setOpen(false); }}>
              {result.reached ? "Применить" : "Применить лучший замер"}
            </Button>}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function App({ embedded = false, onEmbeddedPrefs }) {
  const [prefs, setPrefs] = useState(defaults);
  const prefsRef = useRef(defaults);
  const lastFactor = useRef(4);
  const saveQueue = useRef(Promise.resolve());
  const sourceRefresh = useRef(0);
  const [sources, setSources] = useState([]);
  const [nrAvailable, setNrAvailable] = useState(false);
  const [selected, setSelected] = useState("");
  const [status, setStatus] = useState({ phase: "idle", message: "Выберите источник." });
  const [notice, setNotice] = useState("");
  const [ready, setReady] = useState(!desktop);
  const [pending, setPending] = useState(false);
  const [entrance, setEntrance] = useState(true);
  const [editingHotkey, setEditingHotkey] = useState(false);
  const writingHotkey = useRef(false);
  const systemDark = useMedia(windowsTheme);
  const dark = prefs.theme === "dark" || (prefs.theme === "system" && systemDark);
  const reduced = useMedia(reducedMotion);
  const effects = prefs.motion && !reduced;
  const active = status.phase === "starting" || status.phase === "running";
  const busy = status.phase === "stopping" || pending;
  const dragRegion = active || busy ? undefined : "";
  const { settings } = prefs;
  const nrDisabled = !ready || !nrAvailable || !settings.nr;
  const availableSources = settings.streamer ? sources.filter((item) => !item.id.screen) : sources;
  const selection = availableSources.find((item) => sourceKey(item) === selected);

  useGlass(effects);
  useEffect(() => {
    if (!desktop || !prefs.sound) return;
    let previous = -Infinity;
    function hover(event) {
      if (event.pointerType !== "mouse" || !(event.target instanceof Element)) return;
      const control = event.target.closest("button:not(:disabled), [role=option], .pv-nr-details input:not(:disabled)");
      if (!control || (event.relatedTarget instanceof Node && control.contains(event.relatedTarget))) return;
      const now = performance.now();
      if (now - previous >= 160) {
        previous = now;
        invoke("feedback", { cue: "hover" }).catch(() => {});
      }
    }
    document.addEventListener("pointerover", hover, { passive: true });
    return () => document.removeEventListener("pointerover", hover);
  }, [prefs.sound]);
  useEffect(() => {
    if (desktop && ready && !embedded) invoke("sync_panel_focus").catch(() => {});
  }, [status.phase, ready, embedded]);

  useEffect(() => {
    if (desktop && ready && !embedded) invoke("panel_ready").catch(() => {});
  }, [ready, embedded]);

  useEffect(() => {
    const timer = setTimeout(() => setEntrance(false), 360);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = dark ? "dark" : "light";
    root.dataset.effects = prefs.motion ? "on" : "off";
  }, [dark, prefs.motion]);

  function play(cue) {
    if (desktop && prefsRef.current.sound) invoke("feedback", { cue }).catch(() => {});
  }

  function report(error, audible = true) {
    setNotice(String(error));
    if (audible) play("error");
  }

  async function refreshSources(streamer = prefsRef.current.settings.streamer, preserve = true) {
    if (!desktop) return;
    const generation = ++sourceRefresh.current;
    const [next, nrReady, remembered] = await Promise.all([
      invoke("sources"), invoke("nr_available"), invoke("restore_source", { streamer }),
    ]);
    const options = streamer ? next.filter((item) => !item.id.screen) : next;
    if (sourceRefresh.current !== generation || prefsRef.current.settings.streamer !== streamer) return;
    setNrAvailable(nrReady);
    setSources(next);
    setSelected((current) => {
      if (preserve && options.some((item) => sourceKey(item) === current)) return current;
      const restored = options.find((item) => item.id.handle === remembered?.handle && item.id.screen === remembered?.screen);
      const defaultScreen = !streamer && prefsRef.current.sourceModeScreen ? options.find((item) => item.id.screen) : null;
      return restored || defaultScreen ? sourceKey(restored || defaultScreen) : "";
    });
  }

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    let querying = false;
    let timer;
    let lastPhase = "idle";
    async function refreshStatus() {
      if (querying) return;
      clearTimeout(timer);
      querying = true;
      try {
        const value = await invoke("status");
        lastPhase = value.phase;
        if (alive) {
          setStatus((previous) => Object.keys(value).every((key) => previous[key] === value[key]) ? previous : value);
        }
      } catch (error) {
        if (alive) report(error, false);
      } finally {
        querying = false;
        if (alive) {
          const delay = document.hidden ? 1600 : lastPhase === "idle" || lastPhase === "error" ? 700 : 250;
          timer = setTimeout(refreshStatus, delay);
        }
      }
    }
    invoke("preferences").then((value) => {
      if (!alive) return;
      prefsRef.current = value;
      setPrefs(value);
      lastFactor.current = value.lastMultiplier;
      return refreshSources(value.settings.streamer);
    }).catch((error) => report(error, false)).finally(() => { if (alive) setReady(true); });
    refreshStatus();
    document.addEventListener("visibilitychange", refreshStatus);
    const unlisten = listen("panevanta-error", ({ payload }) => { if (alive) report(payload, false); });
    const settingsListener = listen("panevanta-preferences", ({ payload }) => {
      if (alive) {
        prefsRef.current = payload;
        lastFactor.current = payload.lastMultiplier;
        setPrefs(payload);
      }
    });
    return () => {
      alive = false;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", refreshStatus);
      unlisten.then((stop) => stop()).catch(() => {});
      settingsListener.then((stop) => stop()).catch(() => {});
    };
  }, []);

  const sourceHandle = selection?.id.handle;
  const sourceScreen = selection?.id.screen;
  useEffect(() => {
    if (desktop && ready && !embedded) {
      const source = sourceHandle === undefined ? null : { handle: sourceHandle, screen: sourceScreen };
      invoke("select_source", { source }).catch(report);
    }
  }, [sourceHandle, sourceScreen, ready, embedded]);

  function changePrefs(patch, cue) {
    if (cue) play(cue);
    const next = { ...prefsRef.current, ...patch };
    prefsRef.current = next;
    setPrefs(next);
    onEmbeddedPrefs?.(next);
    setNotice("");
    if (!desktop) return;
    saveQueue.current = saveQueue.current.then(() => invoke("update_preferences", patch))
      .then((warning) => { if (warning) report(warning, false); })
      .catch((error) => report(error));
  }

  function setting(name, value, cue = "select") {
    if (name === "multiplier" && value !== 1) lastFactor.current = value;
    const next = { ...prefsRef.current, settings: { ...prefsRef.current.settings, [name]: value } };
    prefsRef.current = next;
    setPrefs(next);
    onEmbeddedPrefs?.(next);
    setNotice("");
    if (cue) play(cue);
    if (desktop) {
      saveQueue.current = saveQueue.current.then(() => invoke("update_quick", { edit: { field: name, value } }))
        .then(([, warning]) => { if (warning) report(warning, false); })
        .catch(report);
    }
  }

  async function streamerMode(streamer) {
    setting("streamer", streamer, streamer ? "effectOn" : "effectOff");
    setSelected("");
    await saveQueue.current;
    await refreshSources(streamer, false);
  }

  function selectSource(value) {
    const target = availableSources.find((item) => sourceKey(item) === value);
    if (!target) return;
    play("select");
    setNotice("");
    setSelected(value);
    if (!settings.streamer) prefsRef.current = { ...prefsRef.current, sourceModeScreen: target.id.screen };
    invoke("select_source", { source: target.id, remember: true })
      .then((warning) => { if (warning) report(warning, false); })
      .catch(report);
  }

  async function toggle() {
    if (!desktop || busy || (!active && !selection)) return;
    setPending(true);
    setNotice("");
    try {
      await saveQueue.current;
      if (active) {
        await invoke("stop");
      } else {
        const warning = await invoke("select_source", { source: selection.id, remember: true });
        if (warning) report(warning, false);
        await invoke("toggle", { startOnly: true });
      }
      setStatus(await invoke("status"));
    } catch (error) { report(error); }
    finally { setPending(false); }
  }

  async function captureHotkey(event) {
    if (!editingHotkey) return;
    if (event.key === "Tab") { setEditingHotkey(false); return; }
    event.preventDefault();
    if (event.key === "Escape") { setEditingHotkey(false); return; }
    if (event.repeat || writingHotkey.current) return;
    const mods = [event.shiftKey && "Shift", event.ctrlKey && "Ctrl", event.altKey && "Alt", event.metaKey && "Super"].filter(Boolean);
    if (!mods.length || !event.code || /^(Shift|Control|Alt|Meta)(Left|Right)$/.test(event.code)) return;
    const hotkey = [...mods, event.code].join("+");
    writingHotkey.current = true;
    try {
      const warning = desktop ? await invoke("change_hotkey", { hotkey }) : null;
      prefsRef.current = { ...prefsRef.current, hotkey };
      setPrefs(prefsRef.current);
      onEmbeddedPrefs?.(prefsRef.current);
      setNotice(warning || "");
      setEditingHotkey(false);
      play("select");
    } catch (error) { report(error); }
    finally { writingHotkey.current = false; }
  }

  const windowAction = (method, cue) => () => {
    play(cue);
    appWindow?.[method]().catch(report);
  };
  const currentInput = (active && status.input) || settings.input;
  const currentFactor = (active && status.factor) || settings.multiplier;
  const output = currentInput && currentFactor ? currentInput * currentFactor : null;
  const hotkey = prefs.hotkey.replace(/Key(?=[A-Z])/g, "").replace(/Digit(?=\d)/g, "").split("+");
  const vfg = settings.multiplier !== 1;

  return (
    <div className="pv-app" data-embedded={embedded || undefined} data-entrance={entrance && !embedded ? "" : undefined}>
      <header className="pv-titlebar" data-tauri-drag-region={embedded ? undefined : dragRegion} data-glass>
        <div className="pv-brand" data-tauri-drag-region={dragRegion}>
          <img src={brand} alt="" width="43" height="43" draggable="false" />
          <div data-tauri-drag-region={dragRegion}><strong>Panevanta</strong><small>RTX VIDEO</small></div>
        </div>
        <div className="pv-title-actions">
          <IconButton icon={prefs.sound ? Volume2 : VolumeX} label={prefs.sound ? "Выключить звуки" : "Включить звуки"} aria-pressed={prefs.sound} onClick={() => changePrefs({ sound: !prefs.sound })} disabled={!ready} />
          <IconButton icon={Sparkles} label={prefs.motion ? "Выключить визуальные эффекты" : "Включить визуальные эффекты"} aria-pressed={prefs.motion} onClick={() => changePrefs({ motion: !prefs.motion }, prefs.motion ? "effectOff" : "effectOn")} disabled={!ready} />
          <IconButton icon={dark ? Sun : Moon} label={dark ? "Включить светлую тему" : "Включить тёмную тему"} onClick={() => changePrefs({ theme: dark ? "light" : "dark" }, "theme")} disabled={!ready} />
          <InterfaceSettings prefs={prefs} ready={ready} onOpen={() => play("settings")} onTheme={(theme) => changePrefs({ theme }, "theme")} onMotion={(motion) => changePrefs({ motion }, motion ? "effectOn" : "effectOff")} onSound={(sound) => changePrefs({ sound })} onTop={(mainOnTop) => changePrefs({ mainOnTop }, mainOnTop ? "effectOn" : "effectOff")} />
          <div className="pv-window-controls">
            <IconButton icon={Minus} label="Свернуть" onClick={windowAction("minimize", "minimize")} />
            <IconButton icon={Maximize} label="Развернуть или восстановить" onClick={windowAction("toggleMaximize", "maximize")} />
            <IconButton icon={X} label="Свернуть Panevanta в трей" variant="danger" onClick={windowAction("close", "tray")} />
          </div>
        </div>
      </header>

      <main className="pv-main">
        <h1 className="sr-only">Панель управления Panevanta</h1>
        <div className="pv-layout">
          <div className="pv-stack">
            <Card aria-labelledby="source-title" className="pv-source-card">
              <div className="pv-section-head">
                <div className="pv-heading"><Monitor size={20} aria-hidden="true" /><h2 id="source-title">Источник видео</h2></div>
                <div className="pv-source-actions">
                  <TuneDialog ready={desktop && ready && !active && !busy} source={selection} settings={settings}
                    beforeStart={() => saveQueue.current}
                    onApply={(profile) => {
                      if (profile.multiplier !== 1) lastFactor.current = profile.multiplier;
                      changePrefs({ settings: profile }, "select");
                    }} />
                  <IconButton icon={RefreshCw} label="Обновить список источников" disabled={active || busy || !desktop} onClick={() => { play("refresh"); refreshSources().catch(report); }} />
                </div>
              </div>
              <div className="pv-stream-mode" data-enabled={settings.streamer}>
                <span className="pv-stream-icon"><Radio size={17} aria-hidden="true" /></span>
                <div><strong>Режим стримера</strong><small>Отдельное окно для захвата в OBS</small></div>
                <Switch aria-label="Режим стримера" checked={settings.streamer} disabled={!ready || active || busy}
                  onCheckedChange={(value) => streamerMode(value).catch(report)} />
              </div>
              <label className="sr-only" htmlFor="source">Источник захвата</label>
              <Select id="source" value={selection ? selected : ""} placeholder={availableSources.length ? "Выберите окно" : "Нет доступных источников"} onValueChange={selectSource} disabled={active || busy || !availableSources.length}>
                {[[true, "Экраны"], [false, "Окна"]].map(([screen, label]) => {
                  const group = availableSources.filter((item) => item.id.screen === screen);
                  return group.length > 0 && <SelectGroup key={label} label={label}>{group.map((item) => <SelectItem key={sourceKey(item)} value={sourceKey(item)}>{item.label}</SelectItem>)}</SelectGroup>;
                })}
              </Select>
              <div className="pv-source-fields">
                <Field id="input" label="Входная частота" options={fps} value={settings.input} onChange={(v) => setting("input", Number(v))} disabled={!ready} />
                <Field id="height" label="Разрешение обработки" options={heights} value={settings.height} onChange={(v) => setting("height", Number(v))} disabled={!ready} />
              </div>
            </Card>

            <Card aria-labelledby="effects-title" className="pv-effects">
              <div className="pv-section-head"><div className="pv-heading"><Layers3 size={20} aria-hidden="true" /><h2 id="effects-title">Эффекты RTX</h2></div></div>
              <Effect icon={Film} title="Генерация кадров" description="Video Frame Generation" enabled={vfg}
                disabled={!ready} onToggle={(on) => setting("multiplier", on ? lastFactor.current : 1, on ? "effectOn" : "effectOff")}>
                <Field id="multiplier" label="Множитель" options={factors} value={vfg ? settings.multiplier : lastFactor.current} onChange={(v) => setting("multiplier", Number(v))} disabled={!ready || !vfg} />
                <Field id="quality" label="Качество генерации" options={qualities} value={settings.quality} onChange={(v) => setting("quality", Number(v))} disabled={!ready || !vfg} />
              </Effect>
              <Effect icon={Expand} title="Суперразрешение" description="Video Super Resolution" enabled={settings.sr}
                disabled={!ready} onToggle={(value) => setting("sr", value, value ? "effectOn" : "effectOff")}>
                <Field id="scale" label="Масштаб" options={scales} value={settings.scale} onChange={(v) => setting("scale", Number(v))} disabled={!ready || !settings.sr} />
              </Effect>
              <Effect icon={Layers3} title="Расширение диапазона" description="RTX Video HDR" enabled={settings.hdr}
                disabled={!ready} onToggle={(value) => setting("hdr", value, value ? "effectOn" : "effectOff")}>
                <Field id="peak" label="Пиковая яркость" options={peaks} value={settings.peak} onChange={(v) => setting("peak", Number(v))} disabled={!ready || !settings.hdr} />
              </Effect>
              <Effect icon={Sparkles} title="DLSS 5 Neural Rendering" description={nrAvailable ? "Освещение и материалы" : "Требуется NVIDIA runtime DLSS 5 NR"}
                enabled={settings.nr && nrAvailable} disabled={!ready || !nrAvailable}
                onToggle={(value) => setting("nr", value, value ? "effectOn" : "effectOff")}>
                <NrStyle value={settings.nrStyle} onChange={(value) => setting("nrStyle", value)} disabled={nrDisabled} />
                <Field id="nr-resolution" label="Разрешение NR" options={nrResolutions} value={settings.nrResolution}
                  onChange={(v) => setting("nrResolution", Number(v))} disabled={nrDisabled} />
                <Field id="nr-passes" label="Проходы NR" options={nrPasses} value={settings.nrPasses}
                  onChange={(v) => setting("nrPasses", Number(v))} disabled={nrDisabled} />
                <NrLevel id="nr-tone" label="Локальный тон NR" value={settings.nrTone}
                  onCommit={(v) => setting("nrTone", v)} disabled={nrDisabled} />
                <NrLevel id="nr-structure" label="Структура NR" value={settings.nrStructure}
                  onCommit={(v) => setting("nrStructure", v)} disabled={nrDisabled || !settings.nrDetail} />
                <NrLevel id="nr-skin" label="Кожа NR" value={settings.nrSkin}
                  onCommit={(v) => setting("nrSkin", v)} disabled={nrDisabled || !settings.nrDetail} />
                <label className="pv-nr-details">
                  <input type="checkbox" checked={settings.nrAutoMask} disabled={nrDisabled || !settings.nrDetail}
                    onChange={(event) => setting("nrAutoMask", event.target.checked)} />
                  Автоматическая маска
                </label>
                <label className="pv-nr-details">
                  <input type="checkbox" checked={settings.nrUiCorrection} disabled={nrDisabled}
                    onChange={(event) => setting("nrUiCorrection", event.target.checked)} />
                  Коррекция элементов интерфейса
                </label>
                <label className="pv-nr-details">
                  <input type="checkbox" checked={settings.nrDetail} disabled={nrDisabled}
                    onChange={(event) => setting("nrDetail", event.target.checked, event.target.checked ? "effectOn" : "effectOff")} />
                  Улучшать лица и текстуры тела
                </label>
                {(status.nrLoading || status.nrError) && <p className="pv-nr-status" role="status">
                  {status.nrError || "DLSS 5 готовит модель. Обработка видео продолжается."}
                </p>}
              </Effect>
            </Card>
          </div>

          <aside className="pv-stack" aria-label="Управление и диагностика">
            <Card className="pv-session" aria-labelledby="session-title">
              <div className="pv-session-hero" data-phase={status.phase}>
                <img className="pv-prism" src={prism} alt="" draggable="false" />
                <div className="pv-session-header">
                  <h2 id="session-title">Текущий сеанс</h2>
                  <span className="pv-status-chip"><span aria-hidden="true" />{stateLabels[status.phase] || stateLabels.idle}</span>
                </div>
                <div className="pv-target">
                  {output ? <><span>{currentInput}</span><ArrowRight aria-hidden="true" size={22} /><strong>{output}</strong><small>FPS · цель</small></> : <><strong>{settings.multiplier > 1 ? `${settings.multiplier}×` : settings.multiplier === 0 ? "Авто" : "—"}</strong><small>{vfg ? "Режим VFG" : "VFG выключена"}</small></>}
                </div>
                <p className="pv-current-source" title={selection?.label}><Monitor size={14} aria-hidden="true" /><span>{selection?.label || "Источник не выбран"}</span></p>
                {status.autoInput && active && <p className="pv-cadence">{status.cadenceLocked ? `Авто определило ${status.input} FPS` : status.cadenceError ? "Авто-FPS недоступен · выберите FPS вручную" : "Определяю частоту источника…"}</p>}
              </div>
              <div className="pv-session-actions">
                <p className="pv-source-status" data-error={Boolean(notice) || status.phase === "error"} role="status">
                  {notice || (status.phase === "idle" && selection ? "Готово к запуску." : status.message)}
                </p>
                <Button variant="default" size="launch" type="button" onClick={toggle} disabled={!ready || (!active && !selection) || busy}>
                  {active ? <Square size={17} /> : <Play size={17} />}{active ? (status.streamer ? "Остановить вывод" : "Остановить RTX") : busy ? "Подождите…" : settings.streamer ? "Открыть окно для OBS" : "Включить RTX"}
                </Button>
                <div className="pv-hotkey">
                  <Keyboard size={17} aria-hidden="true" />
                  <Button variant="ghost" type="button" className="pv-key-button" onClick={() => { play("select"); setEditingHotkey(true); }} onBlur={() => setEditingHotkey(false)} onKeyDown={captureHotkey} disabled={!ready || active} aria-label="Изменить горячую клавишу">
                    {editingHotkey ? "Нажмите сочетание…" : hotkey.map((key, index) => <kbd key={index}>{key}</kbd>)}
                  </Button>
                </div>
              </div>
            </Card>

            <Card aria-labelledby="metrics-title" className="pv-diagnostics">
              <div className="pv-section-head">
                <div className="pv-heading"><ChartNoAxesCombined size={20} aria-hidden="true" /><h2 id="metrics-title">Телеметрия</h2></div>
                <Switch aria-label="Показывать телеметрию" checked={prefs.metrics} disabled={!ready} onCheckedChange={(metrics) => changePrefs({ metrics }, metrics ? "effectOn" : "effectOff")} />
              </div>
              {prefs.metrics && <>
                <dl className="pv-metrics">
                  <Metric icon={ScanLine} label="Захват" value={displayNumber(status.captureFps)} unit="FPS" />
                  <Metric icon={Activity} label="Вывод" value={displayNumber(status.presentFps)} unit="FPS" />
                  <Metric icon={Clock3} label="До Present" value={displayNumber(status.latencyMs)} unit="мс" />
                </dl>
                <dl className="pv-facts">
                  <div><dt>Дисплей</dt><dd>{status.hz ? `${displayNumber(status.hz)} Гц` : "—"}</dd></div>
                  <div><dt>Обработка</dt><dd>{status.width ? `${status.width} × ${status.height}${status.hdrOutput ? " · HDR" : ""}` : "—"}</dd></div>
                  {status.nrWidth > 0 && <div><dt>DLSS 5 NR</dt><dd>{status.nrWidth} × {status.nrHeight} · {displayNumber(status.nrMs)} мс</dd></div>}
                  <div><dt>Сбросы задержки / пропуски</dt><dd>{status.width ? `${status.resets} / ${status.drops}` : "—"}</dd></div>
                </dl>
              </>}
              <Button variant="glass" type="button" className="w-full" disabled={!status.logAvailable} onClick={() => { play("journal"); invoke("open_log").catch(report); }}>
                <FileText size={15} />Открыть журнал
              </Button>
            </Card>
            <footer className="pv-footer">v{version}</footer>
          </aside>
        </div>
      </main>
    </div>
  );
}

function CompactApp() {
  const [prefs, setPrefs] = useState(defaults);
  const [status, setStatus] = useState({ phase: "starting" });
  const [ready, setReady] = useState(false);
  const [pending, setPending] = useState(false);
  const [notice, setNotice] = useState("");
  const [expanded, setExpanded] = useState(false);
  const systemDark = useMedia(windowsTheme);
  const reduced = useMedia(reducedMotion);
  const dark = prefs.theme === "dark" || (prefs.theme === "system" && systemDark);
  const { settings } = prefs;

  useGlass(prefs.motion && !reduced);
  useEffect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    document.documentElement.dataset.effects = prefs.motion ? "on" : "off";
  }, [dark, prefs.motion]);

  useEffect(() => {
    let active = true;
    let timer;
    let querying = false;
    async function refresh() {
      if (querying) return;
      clearTimeout(timer);
      querying = true;
      try {
        const value = await invoke("status");
        if (active) setStatus(value);
      } catch (error) {
        if (active) setNotice(String(error));
      } finally {
        querying = false;
        if (active) timer = setTimeout(refresh, document.hidden ? 1200 : 240);
      }
    }
    const subscription = listen("panevanta-preferences", ({ payload }) => { if (active) setPrefs(payload); });
    const expand = listen("panevanta-expand", () => {
      invoke("compact_details", { expanded: true }).then(() => {
        if (active) setExpanded(true);
      }).catch((error) => { if (active) setNotice(String(error)); });
    });
    invoke("preferences").then((value) => {
      if (active) { setPrefs(value); setReady(true); }
    }).catch((error) => { if (active) setNotice(String(error)); });
    document.addEventListener("visibilitychange", refresh);
    refresh();
    return () => {
      active = false;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", refresh);
      Promise.all([subscription, expand]).then((handlers) => handlers.forEach((dispose) => dispose())).catch(() => {});
    };
  }, []);

  async function edit(field, value) {
    setPending(true);
    try {
      const [updated, warning] = await invoke("update_quick", { edit: { field, value } });
      setPrefs(updated);
      setNotice(warning || "");
      if (prefs.sound) invoke("feedback", { cue: typeof value === "boolean" ? (value ? "effectOn" : "effectOff") : "select" }).catch(() => {});
    } catch (error) { setNotice(String(error)); }
    finally { setPending(false); }
  }

  async function toggleDetails() {
    try {
      await invoke("compact_details", { expanded: !expanded });
      setExpanded(!expanded);
    } catch (error) { setNotice(String(error)); }
  }

  const disabled = !ready || pending || status.phase === "stopping";
  const effects = [
    ["vfg", "VFG", "Генерация кадров", settings.multiplier !== 1],
    ["sr", "RTX SR", "Суперразрешение", settings.sr],
    ["hdr", "RTX HDR", "Расширение диапазона", settings.hdr],
    ["nr", "DLSS 5", "DLSS 5 Neural Rendering", settings.nr],
  ];
  const message = notice || (status.phase === "error" || status.bypassed ? status.message : "") || status.nrError || (status.nrLoading ? "Загрузка модели DLSS 5…" : "");

  return (
    <div className={expanded ? "pv-compact pv-compact-expanded" : "pv-compact"}>
      <header className="pv-compact-head">
        <div className="pv-compact-handle" onPointerDown={(event) => {
          if (event.button === 0) {
            event.preventDefault();
            invoke("drag_compact").catch((error) => setNotice(String(error)));
          }
        }}>
          <GripHorizontal size={16} aria-hidden="true" />
          <h1>Panevanta</h1>
        </div>
        <span className="pv-compact-phase" data-phase={status.phase}>{status.bypassed ? "Без эффектов" : stateLabels[status.phase] || stateLabels.idle}</span>
        <IconButton label={expanded ? "Свернуть настройки" : "Все настройки"} icon={Settings2}
          aria-expanded={expanded} onClick={toggleDetails} />
        <IconButton label="Свернуть в трей" icon={Minus} onClick={() => invoke("hide_compact").catch((error) => setNotice(String(error)))} />
        <IconButton label="Скрыть компактную панель" icon={X} onClick={() => invoke("hide_compact").catch((error) => setNotice(String(error)))} />
      </header>

      {expanded ? <App embedded onEmbeddedPrefs={setPrefs} /> : <>
      <dl className="pv-metrics pv-compact-metrics">
        <Metric label="Захват" value={displayNumber(status.captureFps)} unit="FPS" icon={ScanLine} />
        <Metric label="Вывод" value={displayNumber(status.presentFps)} unit="FPS" icon={Activity} />
        <Metric label="Задержка" value={displayNumber(status.latencyMs)} unit="мс" icon={Clock3} />
      </dl>

      <section className="pv-compact-effects" aria-label="Быстрые переключатели">
        {effects.map(([key, label, name, checked]) => (
          <div className="pv-compact-effect" data-enabled={checked} key={key}>
            <span>{label}</span><Switch aria-label={name} checked={checked} disabled={disabled} onCheckedChange={(value) => edit(key, value)} />
          </div>
        ))}
      </section>

      <section className="pv-compact-fields" aria-label="Быстрые настройки">
        <Field id="quick-height" label="Обработка RTX" options={heights} value={settings.height} onChange={(v) => edit("height", Number(v))} disabled={disabled} />
        <Field id="quick-nr-resolution" label="Разрешение DLSS 5" options={nrResolutions} value={settings.nrResolution} onChange={(v) => edit("nrResolution", Number(v))} disabled={disabled || !settings.nr} />
        <Field id="quick-nr-style" label="Стиль NR" options={nrStyles} value={settings.nrStyle} onChange={(v) => edit("nrStyle", Number(v))} disabled={disabled || !settings.nr} />
        <label className="pv-compact-detail">
          <input type="checkbox" checked={settings.nrDetail} disabled={disabled || !settings.nr} onChange={(event) => edit("nrDetail", event.target.checked)} />
          Лица и текстуры
        </label>
      </section>

      <footer className="pv-compact-footer">
        <span title={message} role={message ? "status" : undefined}>{message || (status.nrMs ? `DLSS 5 · ${displayNumber(status.nrMs)} мс` : prefs.hotkey.replaceAll("+", " + "))}</span>
        <Button variant="glass" type="button" disabled={!(status.phase === "running" || status.phase === "starting")} onClick={() => invoke("stop").catch((error) => setNotice(String(error)))}><Square size={13} /> Стоп</Button>
      </footer>
      </>}
    </div>
  );
}

function TrayApp() {
  const [prefs, setPrefs] = useState(defaults);
  const [status, setStatus] = useState({ phase: "idle" });
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [ready, setReady] = useState(false);
  const systemDark = useMedia(windowsTheme);
  const reduced = useMedia(reducedMotion);
  const dark = prefs.theme === "dark" || (prefs.theme === "system" && systemDark);
  const active = status.phase === "running" || status.phase === "starting";

  useGlass(prefs.motion && !reduced);
  useEffect(() => {
    document.documentElement.dataset.theme = dark ? "dark" : "light";
    document.documentElement.dataset.effects = prefs.motion ? "on" : "off";
  }, [dark, prefs.motion]);

  useEffect(() => {
    let alive = true;
    let timer;
    async function refresh() {
      const value = await invoke("status").catch(() => null);
      if (!alive) return;
      if (value) setStatus(value);
      timer = setTimeout(refresh, document.hidden ? 1200 : 400);
    }
    const subscription = listen("panevanta-preferences", ({ payload }) => { if (alive) setPrefs(payload); });
    invoke("preferences").then((value) => {
      if (!alive) return;
      setPrefs(value);
      setReady(true);
    }).catch((error) => setNotice(String(error)));
    refresh();
    function close(event) {
      if (event.key === "Escape") invoke("tray_action", { action: "hide" }).catch(() => {});
    }
    window.addEventListener("keydown", close);
    return () => {
      alive = false;
      clearTimeout(timer);
      window.removeEventListener("keydown", close);
      subscription.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (ready) invoke("tray_ready").catch((error) => setNotice(String(error)));
  }, [ready]);

  async function edit(field, value) {
    setBusy(true);
    try {
      const [updated, warning] = await invoke("update_quick", { edit: { field, value } });
      setPrefs(updated);
      setNotice(warning || "");
      if (updated.sound) invoke("feedback", { cue: value ? "effectOn" : "effectOff" }).catch(() => {});
    } catch (error) { setNotice(String(error)); }
    finally { setBusy(false); }
  }

  function action(key) {
    setNotice("");
    invoke("tray_action", { action: key }).catch((error) => setNotice(String(error)));
  }

  const effects = [
    ["vfg", "VFG", "Генерация кадров", prefs.settings.multiplier !== 1, Layers3],
    ["sr", "RTX SR", "Суперразрешение", prefs.settings.sr, Expand],
    ["hdr", "RTX HDR", "Динамический диапазон", prefs.settings.hdr, Sun],
    ["nr", "DLSS 5", "Neural Rendering", prefs.settings.nr, Sparkles],
  ];
  return (
    <main className="pv-tray" aria-label="Меню системного трея">
      <header className="pv-tray-head">
        <img src={brand} alt="" width="34" height="34" />
        <div><h1>Panevanta</h1><span>{status.bypassed ? "Без эффектов" : stateLabels[status.phase]}</span></div>
        <Button variant="glass" onClick={() => action("toggle")} disabled={status.phase === "stopping"}>
          {active ? <Square size={13} /> : <Play size={13} />}{active ? "Стоп" : "Запуск"}
        </Button>
        <IconButton label="Закрыть меню трея" icon={X} onClick={() => action("hide")} />
      </header>
      <section className="pv-tray-effects" aria-label="Эффекты">
        {effects.map(([key, label, title, checked, Icon]) => (
          <div className="pv-tray-effect" key={key} data-enabled={checked}>
            <Icon size={17} aria-hidden="true" /><span>{label}<small>{title}</small></span>
            <Switch aria-label={label} checked={checked} disabled={busy} onCheckedChange={(value) => edit(key, value)} />
          </div>
        ))}
      </section>
      <nav className="pv-tray-actions" aria-label="Окна">
        <button className="pv-tray-primary" onClick={() => action("compact")}><Scan size={16} />Компактная панель</button>
        <button onClick={() => action("settings")}><Settings2 size={16} />Все настройки</button>
        <button onClick={() => action("main")}><PanelTop size={16} />Основное окно</button>
        <button className="pv-tray-exit" onClick={() => action("quit")}><X size={16} />Выход</button>
      </nav>
      {notice && <p className="pv-tray-notice" role="status">{notice}</p>}
    </main>
  );
}

const windowLabel = appWindow?.label;
if (windowLabel === "compact" || windowLabel === "tray") document.documentElement.dataset.window = windowLabel;
createRoot(document.getElementById("root")).render(windowLabel === "compact" ? <CompactApp /> : windowLabel === "tray" ? <TrayApp /> : <App />);
