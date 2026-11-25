namespace Veyra;

internal sealed class ControlPanel : Form
{
    private readonly ComboBox sources = Choice();
    private const int AutoMultiplierIndex = 6;
    private readonly ComboBox multiplier = Choice("Выкл.", "2×", "3×", "4×", "5×", "6×", "Авто");
    private readonly ComboBox peak = Choice("Авто", "250", "350", "500", "750", "1000");
    private readonly TextBox hotkeyBox = new() { ReadOnly = true, Width = 145 };
    private string hotkeyText = "Shift + Alt + S";
    private uint hotkeyMods = 5;
    private uint hotkeyKey = (uint)Keys.S;
    private Cadence? cadence;
    private double displayHz;
    private bool reconfiguring;
    private bool smokeFinishing;
    private int fpsChanges;
    private int restarts, lagResets;
    private long settingsChangedAt, lastRestartAt;
    private double SmokeSeconds => double.TryParse(Environment.GetEnvironmentVariable("VEYRA_SMOKE_SECONDS"), out double seconds) ? Math.Clamp(seconds, 16, 90) : 16;
    private readonly ComboBox resolution = Choice("Исходное", "До 1080p", "До 900p", "До 720p");
    private readonly CheckBox showMetrics = new() { Text = "Показать FPS и задержку", AutoSize = true };
    private readonly Label telemetry = new() { AutoSize = true, MaximumSize = new(480, 0), Visible = false };
    private Performance performance = new();
    private readonly ComboBox quality = Choice("Низкое", "Среднее", "Высокое");
    private readonly ComboBox scale = Choice("1.01", "1.25", "1.5", "2");
    private readonly ComboBox fps = Choice("Авто", "24", "25", "30", "48", "50", "60", "72", "90", "120");
    private readonly CheckBox sr = new() { Text = "RTX Super Resolution", AutoSize = true, Checked = true };
    private readonly CheckBox hdr = new() { Text = "RTX HDR", AutoSize = true, Checked = true };
    private readonly Button start = new() { Text = "Включить RTX", AutoSize = true };
    private readonly Button refresh = new() { Text = "Обновить", AutoSize = true };
    private readonly Button diagnostics = new() { Text = "Журнал", AutoSize = true, Enabled = false };
    private readonly Label status = new() { AutoSize = true, MaximumSize = new(480, 0) };
    private readonly System.Windows.Forms.Timer monitor = new() { Interval = 120 };
    private readonly string root;
    private string log = "";
    private int logSequence;
    private readonly string? smoke;
    private bool UsesExternalSource => smoke is "windows-media" or "browser-window";
    private bool VerifyInput => !UsesExternalSource && Environment.GetEnvironmentVariable("VEYRA_SMOKE_SETTINGS_ONLY") != "1";
    private Pipeline? running;
    private Source? active;
    private bool closing;
    private bool shortcut;
    private Form? sample;
    private readonly System.Diagnostics.Stopwatch smokeClock = new();
    private int ticks;
    private int clicked;
    private Button? testButton;
    private bool ready;
    private Effects? requested;
    private int smokeStep;

    public ControlPanel(string root, string? smoke)
    {
        this.root = root;
        this.smoke = smoke;
        Text = "Panevanta";
        Icon = Icon.ExtractAssociatedIcon(Environment.ProcessPath!);
        Font = new Font("Segoe UI", 10);
        BackColor = Color.FromArgb(19, 21, 29);
        ForeColor = Color.FromArgb(240, 241, 248);
        StartPosition = FormStartPosition.CenterScreen;
        MinimumSize = new(535, 350);
        AutoSize = true;
        AutoSizeMode = AutoSizeMode.GrowAndShrink;
        TopMost = true;

        var layout = new TableLayoutPanel
        {
            AutoSize = true, Dock = DockStyle.Fill, ColumnCount = 3, Padding = new(18), GrowStyle = TableLayoutPanelGrowStyle.AddRows
        };
        var heading = new TableLayoutPanel { AutoSize = true, ColumnCount = 2, RowCount = 2, Margin = new(0, 0, 0, 14) };
        if (Icon is not null)
        {
            using var emblem = new Icon(Icon, 48, 48);
            var logo = new PictureBox { Image = emblem.ToBitmap(), Size = new(48, 48), SizeMode = PictureBoxSizeMode.CenterImage, Margin = new(0, 2, 12, 0) };
            logo.Disposed += (_, _) => logo.Image?.Dispose();
            heading.Controls.Add(logo, 0, 0);
            heading.SetRowSpan(logo, 2);
        }
        heading.Controls.Add(new Label { Text = "Panevanta", Font = new Font(Font.FontFamily, 22, FontStyle.Bold), AutoSize = true, Margin = new(0) }, 1, 0);
        heading.Controls.Add(new Label { Text = "Обработка окна и экрана", ForeColor = Color.FromArgb(178, 186, 209), AutoSize = true, Margin = new(1, 0, 0, 0) }, 1, 1);
        layout.Controls.Add(heading, 0, 0);
        layout.SetColumnSpan(heading, 3);
        Add(layout, "Источник", sources, 1);
        sources.Width = 290;
        layout.Controls.Add(refresh, 2, 1);
        Add(layout, "VFG · множитель", multiplier, 2);
        Add(layout, "Качество VFG", quality, 3);
        Add(layout, "Частота захвата", fps, 4);
        Add(layout, "Масштаб SR", scale, 5);
        var options = new FlowLayoutPanel { AutoSize = true, WrapContents = false };
        options.Controls.AddRange([sr, hdr]);
        layout.Controls.Add(options, 1, 6);
        layout.SetColumnSpan(options, 2);
        var buttons = new FlowLayoutPanel { AutoSize = true, WrapContents = false };
        buttons.Controls.AddRange([start, diagnostics]);
        layout.Controls.Add(buttons, 1, 7);
        layout.SetColumnSpan(buttons, 2);
        Add(layout, "Пик HDR (нит)", peak, 8);
        Add(layout, "Горячая клавиша", hotkeyBox, 9);
        Add(layout, "Размер обработки", resolution, 10);
        layout.Controls.Add(showMetrics, 1, 11);
        layout.SetColumnSpan(showMetrics, 2);
        layout.Controls.Add(telemetry, 0, 12);
        layout.SetColumnSpan(telemetry, 3);
        layout.Controls.Add(status, 0, 13);
        layout.SetColumnSpan(status, 3);
        foreach (var button in new[] { start, refresh, diagnostics })
        {
            button.FlatStyle = FlatStyle.Flat;
            button.FlatAppearance.BorderColor = Color.FromArgb(80, 89, 119);
            button.Padding = new(8, 3, 8, 3);
            button.BackColor = Color.FromArgb(32, 37, 53);
        }
        start.BackColor = Color.FromArgb(173, 189, 255);
        start.ForeColor = Color.FromArgb(16, 23, 46);
        telemetry.ForeColor = Color.FromArgb(173, 189, 255);
        Controls.Add(layout);
        multiplier.SelectedIndex = 3;
        fps.SelectedIndex = 0;
        peak.SelectedIndex = 0;
        resolution.SelectedIndex = 1;
        showMetrics.CheckedChanged += (_, _) => telemetry.Visible = showMetrics.Checked;
        hotkeyBox.KeyDown += ChangeHotkey;
        quality.SelectedIndex = 1;
        scale.SelectedIndex = 0;
        foreach (var choice in new[] { multiplier, quality, fps, scale, peak, resolution })
            choice.SelectedIndexChanged += (_, _) => MarkSettingsChanged();
        sr.CheckedChanged += (_, _) => MarkSettingsChanged();
        hdr.CheckedChanged += (_, _) => MarkSettingsChanged();
        refresh.Click += (_, _) => RefreshSources();
        start.Click += async (_, _) => await Toggle();
        diagnostics.Click += (_, _) =>
        {
            if (File.Exists(log)) System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(log) { UseShellExecute = true });
        };
        monitor.Tick += async (_, _) => await Inspect();
        Shown += async (_, _) =>
        {
            Native.SetWindowDisplayAffinity(Handle, 0x11);
            LoadHotkey();
            shortcut = Native.RegisterHotKey(Handle, 1, hotkeyMods | 0x4000, hotkeyKey);
            hotkeyBox.Text = hotkeyText;
            status.Text = shortcut ? $"{hotkeyText} · включить или выключить" : "Выберите источник и нажмите «Включить RTX».";
            if (smoke is not null)
            {
                smokeClock.Start();
                if (smoke is not ("windows-media" or "browser-window")) MakeSample();
                bool auto = smoke is "auto" or "auto60" or "browser-window";
                showMetrics.Checked = true;
                fps.SelectedIndex = auto ? 0 : 3;
                multiplier.SelectedIndex = smoke is "auto" or "auto60" ? AutoMultiplierIndex : smoke == "browser-window" ? 3 : 1;
            }
            RefreshSources();
            if (smoke is not null)
            {
                if (smoke is "window" or "auto" or "auto60" or "live")
                    sources.SelectedItem = sources.Items.Cast<Source>().First(x => x.Handle == sample!.Handle);
                if (int.TryParse(Environment.GetEnvironmentVariable("VEYRA_SMOKE_MULTIPLIER"), out int request) && request is >= 2 and <= 6)
                    multiplier.SelectedIndex = request - 1;
                if (Environment.GetEnvironmentVariable("VEYRA_SMOKE_FPS") is string fixedRate && fps.Items.Contains(fixedRate))
                    fps.SelectedItem = fixedRate;
                if (int.TryParse(Environment.GetEnvironmentVariable("VEYRA_SMOKE_RESOLUTION"), out int size) && size is >= 0 and <= 3) resolution.SelectedIndex = size;
                if (int.TryParse(Environment.GetEnvironmentVariable("VEYRA_SMOKE_SCALE"), out int srScale) && srScale is >= 0 and <= 3) scale.SelectedIndex = srScale;
                if (smoke == "browser-window")
                {
                    if (!long.TryParse(Environment.GetEnvironmentVariable("VEYRA_TEST_WINDOW"), out long handle) ||
                        sources.Items.Cast<Source>().FirstOrDefault(x => x.Screen is null && x.Handle == (nint)handle) is not Source browser)
                    {
                        FinishSmoke("Browser window missing.", 1);
                        return;
                    }
                    sources.SelectedItem = browser;
                    sr.Checked = false;
                    hdr.Checked = false;
                }
                if (smoke == "windows-media")
                {
                    try
                    {
                        var path = Environment.GetEnvironmentVariable("VEYRA_TEST_MEDIA") ?? throw new InvalidOperationException("Test file missing.");
                        var file = await Windows.Storage.StorageFile.GetFileFromPathAsync(path);
                        var launch = new Windows.System.LauncherOptions
                        {
                            TargetApplicationPackageFamilyName = "Microsoft.ZuneMusic_8wekyb3d8bbwe"
                        };
                        if (!await Windows.System.Launcher.LaunchFileAsync(file, launch))
                            throw new InvalidOperationException("Media Player did not accept the file.");
                        Source? target = null;
                        for (int i = 0; i < 20 && target is null; i++)
                        {
                            await Task.Delay(350);
                            foreach (var source in Native.Windows(Handle))
                            {
                                Native.GetWindowThreadProcessId(source.Handle, out uint id);
                                try
                                {
                                    using var process = System.Diagnostics.Process.GetProcessById((int)id);
                                    if (process.ProcessName.Equals("Microsoft.Media.Player", StringComparison.OrdinalIgnoreCase) ||
                                        (process.ProcessName == "ApplicationFrameHost" && source.Name is "Медиаплеер" or "Media Player"))
                                    {
                                        target = source;
                                        break;
                                    }
                                }
                                catch (ArgumentException) { }
                            }
                        }
                        if (target is null) throw new InvalidOperationException("Media Player window missing.");
                        sources.Items.Add(target);
                        sources.SelectedItem = target;
                        fps.SelectedIndex = 0;
                        smokeClock.Restart();
                    }
                    catch (Exception ex) { FinishSmoke(ex.ToString(), 1); return; }
                }
                await Toggle();
            }
        };
    }

    private static ComboBox Choice(params string[] values)
    {
        var box = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = 145 };
        box.Items.AddRange(values);
        return box;
    }

    private static void Add(TableLayoutPanel panel, string label, Control control, int row)
    {
        panel.Controls.Add(new Label { Text = label, Anchor = AnchorStyles.Left, AutoSize = true }, 0, row);
        panel.Controls.Add(control, 1, row);
    }

    private void RefreshSources()
    {
        var previous = sources.SelectedItem as Source;
        sources.Items.Clear();
        foreach (var screen in Screen.AllScreens)
        {
            var rect = screen.Bounds;
            var handle = Native.MonitorFromPoint(new Point(rect.Left + 1, rect.Top + 1), 2);
            sources.Items.Add(new Source($"Экран {sources.Items.Count + 1} · {rect.Width} × {rect.Height}", handle, screen));
        }
        foreach (var source in Native.Windows(Handle)) sources.Items.Add(source);
        var match = sources.Items.Cast<Source>().FirstOrDefault(s => s.Handle == previous?.Handle && s.Screen?.DeviceName == previous?.Screen?.DeviceName);
        if (match is not null) sources.SelectedItem = match;
        else if (sources.Items.Count > 0) sources.SelectedIndex = 0;
    }

    private async Task Toggle()
    {
        if (reconfiguring || closing) return;
        if (running is not null)
        {
            await Stop("Обработка остановлена.");
            return;
        }
        if (sources.SelectedItem is not Source source) return;
        try
        {
            var display = Native.DisplaySettings((source.Screen ?? Screen.FromHandle(source.Handle)).DeviceName);
            displayHz = display.Hz;
            bool automatic = fps.SelectedIndex == 0;
            int input = automatic ? 30 : int.Parse(fps.Text);
            var effects = ReadEffects(input, display.WhiteScale, display.Hdr);
            if (multiplier.SelectedIndex == 0 && !effects.Sr && !effects.Hdr)
                throw new InvalidOperationException("Включите хотя бы один эффект.");
            fpsChanges = restarts = lagResets = 0;
            StartPipeline(source, effects);
            if (automatic) cadence = new Cadence(source);
            status.Text = "Запуск захвата и RTX…";
            diagnostics.Enabled = true;
            SetControls(true);
            monitor.Start();
        }
        catch (Exception ex)
        {
            if (running is not null) await Stop(ex.Message);
            status.Text = ex.Message;
            if (smoke is not null) FinishSmoke(ex.ToString(), 1);
        }
    }

    private int SelectedMultiplier(int input) => multiplier.SelectedIndex == AutoMultiplierIndex
        ? Cadence.Multiplier(input, displayHz)
        : multiplier.SelectedIndex + 1;

    private void MarkSettingsChanged() => settingsChangedAt = System.Diagnostics.Stopwatch.GetTimestamp();

    private Effects ReadEffects(int input, float white, bool displayHdr) => new(
        SelectedMultiplier(input), quality.SelectedIndex switch { 0 => "low", 2 => "high", _ => "medium" },
        sr.Checked, hdr.Checked, input,
        scale.Text, peak.SelectedIndex == 0 ? "auto" : peak.Text, white,
        resolution.SelectedIndex switch { 1 => 1080, 2 => 900, 3 => 720, _ => 0 }, displayHdr);

    private void StartPipeline(Source source, Effects effects, Size? sourceSize = null)
    {
        log = Path.Combine(Path.GetTempPath(), $"panevanta-{Environment.ProcessId}-{++logSequence}.log");
        running = new Pipeline(source, effects, root, log, sourceSize);
        active = source;
        ticks = 0;
        ready = false;
        requested = effects;
        performance = new Performance();
        lastRestartAt = System.Diagnostics.Stopwatch.GetTimestamp();
    }

    private static string HotkeyPath => Path.Combine(AppContext.BaseDirectory, "settings", "hotkey.txt");
    private static string PreviousHotkeyPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Veyra Desktop", "hotkey.txt");

    private void LoadHotkey()
    {
        try
        {
            var path = File.Exists(HotkeyPath) ? HotkeyPath : PreviousHotkeyPath;
            if (!File.Exists(path)) return;
            var parts = File.ReadAllText(path).Split(',');
            if (parts.Length == 2 && uint.TryParse(parts[0], out uint mods) && mods is >= 1 and <= 7 &&
                uint.TryParse(parts[1], out uint key) && key is >= 8 and <= 254)
            {
                hotkeyMods = mods;
                hotkeyKey = key;
                hotkeyText = FormatHotkey(mods, key);
            }
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private static string FormatHotkey(uint mods, uint key) => string.Join(" + ", new[]
    {
        (mods & 4) != 0 ? "Shift" : null, (mods & 2) != 0 ? "Ctrl" : null,
        (mods & 1) != 0 ? "Alt" : null, ((Keys)key).ToString()
    }.Where(part => part is not null));

    private void ChangeHotkey(object? sender, KeyEventArgs e)
    {
        e.SuppressKeyPress = true;
        uint mods = (e.Shift ? 4u : 0) | (e.Control ? 2u : 0) | (e.Alt ? 1u : 0);
        if (mods == 0 || e.KeyCode is Keys.ShiftKey or Keys.Menu or Keys.ControlKey or Keys.None) return;
        uint key = (uint)e.KeyCode;
        if (shortcut) Native.UnregisterHotKey(Handle, 1);
        if (!Native.RegisterHotKey(Handle, 1, mods | 0x4000, key))
        {
            shortcut = Native.RegisterHotKey(Handle, 1, hotkeyMods | 0x4000, hotkeyKey);
            status.Text = "Эта комбинация занята. Выберите другую.";
            return;
        }
        shortcut = true;
        hotkeyMods = mods;
        hotkeyKey = key;
        hotkeyBox.Text = hotkeyText = FormatHotkey(mods, key);
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(HotkeyPath)!);
            File.WriteAllText(HotkeyPath, $"{mods},{key}");
            status.Text = $"{hotkeyText} · включить или выключить";
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            status.Text = "Комбинация изменена для текущего запуска. Сохранить её не удалось.";
        }
    }

    private void SetControls(bool busy)
    {
        sources.Enabled = refresh.Enabled = !busy;
        start.Text = busy ? "Выключить RTX" : "Включить RTX";
    }

    private async Task Restart(Pipeline session, Source source, Effects effects, string message, Size? size = null)
    {
        reconfiguring = true;
        start.Enabled = false;
        status.Text = message;
        telemetry.Text = "";
        running = null;
        try
        {
            await session.Close();
            if (size is not null) await Task.Delay(450);
            if (closing) return;
            StartPipeline(source, effects, size);
            restarts++;
        }
        catch (Exception ex)
        {
            await Stop(ex.Message);
            if (smoke is not null) FinishSmoke(ex.ToString(), 1);
        }
        finally
        {
            reconfiguring = false;
            if (!closing) start.Enabled = true;
        }
    }

    private async Task Inspect()
    {
        var session = running;
        var source = active;
        if (session is null || source is null) return;
        if (reconfiguring || smokeFinishing) return;
        ticks++;
        if (session.Capture.IsCompleted)
        {
            if (session.Capture.Exception?.GetBaseException() is SourceSizeChanged resized &&
                resized.Dimensions.Width >= 16 && resized.Dimensions.Height >= 16 &&
                (source.Screen is not null || Native.IsWindow(source.Handle)))
            {
                await Restart(session, source, requested!, "Размер источника изменился · подстраиваюсь…", resized.Dimensions);
                if (!closing && running is not null && cadence is { Rate: 0 })
                {
                    cadence.Dispose();
                    cadence = new Cadence(source);
                }
                return;
            }
            var error = (smoke is null ? session.Capture.Exception?.GetBaseException().Message : session.Capture.Exception?.GetBaseException().ToString()) ?? "Захват завершился.";
            await Stop(error);
            if (smoke is not null) FinishSmoke(error, 1);
            return;
        }

        var effects = requested!;
        if (smoke == "live" && ready)
        {
            switch (smokeStep)
            {
                case 0 when smokeClock.Elapsed.TotalSeconds >= 5:
                    multiplier.SelectedIndex = 2;
                    smokeStep++;
                    break;
                case 1 when effects.Multiplier == 3 && smokeClock.Elapsed.TotalSeconds >= 9:
                    hdr.Checked = false;
                    smokeStep++;
                    break;
                case 2 when !effects.Hdr && smokeClock.Elapsed.TotalSeconds >= 14:
                    hdr.Checked = true;
                    smokeStep++;
                    break;
            }
        }

        bool settingsSettled = System.Diagnostics.Stopwatch.GetElapsedTime(settingsChangedAt).TotalMilliseconds >= 250;
        if (settingsSettled)
        {
            if (fps.SelectedIndex == 0) cadence ??= new Cadence(source);
            else if (cadence is not null)
            {
                cadence.Dispose();
                cadence = null;
            }
        }
        int input = fps.SelectedIndex == 0 ? (cadence?.Rate is > 0 ? cadence.Rate : effects.Fps) : int.Parse(fps.Text);
        float white = effects.WhiteScale;
        bool displayHdr = effects.DisplayHdr;
        if (ticks % 32 == 0)
        {
            var settings = Native.DisplaySettings((source.Screen ?? Screen.FromHandle(source.Handle)).DeviceName);
            displayHz = settings.Hz;
            white = settings.WhiteScale;
            displayHdr = settings.Hdr;
        }
        var adjusted = ReadEffects(input, white, displayHdr);
        if (settingsSettled && adjusted != effects)
        {
            if (input != effects.Fps) fpsChanges++;
            if (multiplier.SelectedIndex == 0 && !adjusted.Sr && !adjusted.Hdr)
                await Stop("Все эффекты выключены.");
            else
                await Restart(session, source, adjusted, "Применяю настройки · сбрасываю очередь…");
            return;
        }

        if (!ready && ticks % 4 == 0 && File.Exists(log))
        {
            using var file = new FileStream(log, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            using var reader = new StreamReader(file);
            var startupLog = reader.ReadToEnd();
            if (startupLog.Contains("Panevanta native") && session.Captured > 0)
            {
                ready = true;
                bool hdrOutput = startupLog.Contains("Output color space RGB_FULL_G2084_NONE_P2020");
                status.Text = requested!.Hdr && !hdrOutput
                    ? "Обработка работает · включите HDR в настройках дисплея Windows."
                    : (fps.SelectedIndex == 0 ? "Авто · " : "") + $"{requested.Fps} → {requested.Fps * requested.Multiplier} FPS · {displayHz:0.#} Гц"
                        + (requested.PreserveHdr && hdrOutput ? " · HDR источника" : "") + (shortcut ? $" · {hotkeyText}" : "");
            }
        }

        if (!ready && ticks > 175)
        {
            const string error = "Движок не показал первый кадр. Проверьте журнал.";
            await Stop(error);
            if (smoke is not null) FinishSmoke(error, 1);
            return;
        }

        if (ticks % 5 == 0 && File.Exists(session.Log))
        {
            performance.Read(session, effects.Fps);
            if (ready && settingsSettled && performance.NeedsReset(effects.Fps) &&
                System.Diagnostics.Stopwatch.GetElapsedTime(lastRestartAt).TotalSeconds >= 8)
            {
                lagResets++;
                await Restart(session, source, effects, "Обнаружена задержка · сбрасываю очередь…");
                return;
            }
            string cadenceText = cadence is null ? "" : cadence.Rate > 0 ? $" · Авто зафиксировано: {cadence.Rate}" : " · Авто: поиск";
            var age = performance.Age is double ms ? $"≈ {ms:0} мс" : "—";
            var capturedFps = performance.CaptureFps?.ToString("0.#") ?? "—";
            var presentFps = performance.PresentFps?.ToString("0.#") ?? "—";
            string autoMultiplier = multiplier.SelectedIndex == AutoMultiplierIndex ? $" · авто VFG, дисплей {displayHz:0.#} Гц" : "";
            telemetry.Text = $"Режим {effects.Fps} × {effects.Multiplier} → {effects.Fps * effects.Multiplier} FPS{autoMultiplier}{cadenceText}\n"
                + $"Захват {capturedFps} FPS · Вывод (Present) {presentFps} FPS\n"
                + $"Кадр до Present {age} · пропуски вывода {performance.Drops} · {session.Width} × {session.Height} · {session.Transfer}";
        }

        bool canShow = ready;
        if (source.Screen is null)
        {
            if (!Native.IsWindow(source.Handle))
            {
                await Stop("Окно источника закрыто.");
                return;
            }
            var foreground = Native.GetForegroundWindow();
            canShow &= !Native.IsIconic(source.Handle) && (foreground == source.Handle || foreground == Handle || smoke is not null);
            var bounds = source.Bounds;
            if (!bounds.IsEmpty && session.Overlay.Bounds != bounds)
                session.Overlay.Bounds = bounds;
        }

        if (canShow && !session.Overlay.Visible)
        {
            Native.DisableChildInput(session.Overlay.Handle);
            session.Overlay.Show();
            BringToFront();
        }
        else if (!canShow && session.Overlay.Visible) session.Overlay.Hide();

        if (smoke is not null && VerifyInput && clicked == 0 && smokeClock.Elapsed.TotalSeconds is >= 11 and < 13 && session.Overlay.Visible && testButton is not null)
        {
            sample!.Activate();
            Native.RaiseOverlay(session.Overlay.Handle);
            var position = testButton.PointToScreen(new Point(25, 12));
            var hitWindow = Native.WindowFromPoint(position);
            if (Native.GetAncestor(hitWindow, 2) == sample.Handle)
            {
                var previousPosition = Cursor.Position;
                Native.TestClick(position);
                await Task.Delay(120);
                Cursor.Position = previousPosition;
            }
        }

        if (smoke is not null && smokeClock.Elapsed.TotalSeconds >= SmokeSeconds)
        {
            var rect = sample?.Bounds ?? source.Bounds;
            var hit = Native.WindowFromPoint(new Point(rect.Left + 24, rect.Top + 44));
            bool inputPasses = clicked > 0 || !VerifyInput;
            bool registered = shortcut;
            var result = $"shortcut={registered} {hotkeyText} detected={cadence?.Rate ?? 0} fpsChanges={fpsChanges} restarts={restarts} lagResets={lagResets} fps={requested?.Fps} multiplier={requested?.Multiplier} hdr={requested?.Hdr} hz={displayHz} capture={session.Captured} captureFps={performance.CaptureFps:F1} presentFps={performance.PresentFps:F1} overlay={session.Overlay.Visible} inputPasses={inputPasses} hit={hit} hitRoot={Native.GetAncestor(hit, 2)} source={sample?.Handle ?? source.Handle} output={session.Overlay.Handle} control={Handle} renderMean={performance.MeanPresent:F1} meanAge={performance.MeanAge:F1} ageSamples={performance.AgeSamples} drops={performance.Drops} backwards={performance.Backwards} size={session.Width}x{session.Height} transfer={session.Transfer} rendererPid={session.ProcessId} log={log}";
            bool passed = session.Captured > 30 && session.Overlay.Visible && inputPasses && (smoke switch
            {
                "auto" => requested?.Fps == 24 && requested.Multiplier == SelectedMultiplier(24),
                "auto60" => requested?.Fps == 60 && requested.Multiplier == SelectedMultiplier(60),
                "live" => smokeStep == 3 && restarts >= 3 && requested is { Multiplier: 3, Hdr: true },
                _ => true
            });
            smokeFinishing = true;
            sample?.Activate();
            await Task.Delay(100);
            bool testForeground = sample is not null && Native.GetForegroundWindow() == sample.Handle;
            if (registered && testForeground && VerifyInput)
            {
                Native.TestHotkey(hotkeyMods, hotkeyKey);
                await Task.Delay(400);
            }
            bool hotkeyWorked = running is null || !VerifyInput;
            if (running is not null) await Stop(result);
            result += $" hotkeyStop={hotkeyWorked} hotkeyTestForeground={testForeground} peak={requested?.Peak} whiteScale={requested?.WhiteScale:F2}";
            FinishSmoke(result, passed && hotkeyWorked ? 0 : 1);
        }
    }

    private async Task Stop(string message)
    {
        monitor.Stop();
        cadence?.Dispose();
        cadence = null;
        var previous = running;
        running = null;
        active = null;
        start.Enabled = false;
        if (previous is not null) await previous.Close();
        SetControls(false);
        start.Enabled = true;
        telemetry.Text = "";
        status.Text = message;
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == 0x0312 && m.WParam == 1 && !hotkeyBox.Focused) _ = Toggle();
        base.WndProc(ref m);
    }

    protected override async void OnFormClosing(FormClosingEventArgs e)
    {
        if (reconfiguring) { e.Cancel = true; return; }
        if (!closing && running is not null)
        {
            e.Cancel = true;
            await Stop("Обработка остановлена.");
            closing = true;
            Close();
            return;
        }
        monitor.Dispose();
        if (shortcut) Native.UnregisterHotKey(Handle, 1);
        sample?.Dispose();
        base.OnFormClosing(e);
    }

    private void MakeSample()
    {
        sample = new Form { Text = "Veyra motion sample", ClientSize = new(720, 400), StartPosition = FormStartPosition.Manual, Location = new(24, 200), TopMost = true };
        if (Environment.GetEnvironmentVariable("VEYRA_SMOKE_LARGE") == "1")
            sample.ClientSize = new(1920, 1080);
        var stripe = new Panel { BackColor = Color.Gold, Bounds = new(0, 90, 80, 110) };
        testButton = new Button { Text = "Проверка", Bounds = new(20, 20, 96, 30) };
        testButton.Click += (_, _) => clicked++;
        sample.Controls.AddRange([stripe, testButton]);
        bool precise = smoke is "auto" or "auto60";
        if (precise) stripe.Bounds = new(220, 110, 270, 220);
        sample.Show();
        var timerStop = new CancellationTokenSource();
        sample.Disposed += (_, _) => timerStop.Cancel();
        _ = Task.Run(async () =>
        {
            var token = timerStop.Token;
            Native.timeBeginPeriod(1);
            try
            {
                int rate = smoke == "auto60" ? 60 : smoke == "auto" ? 24 : 30;
                var time = System.Diagnostics.Stopwatch.StartNew();
                int previous = -1;
                while (!token.IsCancellationRequested)
                {
                    int frame = (int)(time.Elapsed.TotalSeconds * rate);
                    if (frame != previous)
                    {
                        previous = frame;
                        BeginInvoke(() =>
                        {
                            if (stripe.IsDisposed) return;
                            if (precise) stripe.BackColor = Color.FromArgb(30, 30 + frame % 11 * 19, 110);
                            else stripe.Left = frame * 13 % 630;
                            stripe.Update();
                        });
                    }
                    await Task.Delay(1, token);
                }
            }
            catch (OperationCanceledException) { }
            catch (InvalidOperationException) when (token.IsCancellationRequested || IsDisposed) { }
            finally { Native.timeEndPeriod(1); timerStop.Dispose(); }
        });
    }

    private void FinishSmoke(string message, int code)
    {
        File.WriteAllText(Path.Combine(Environment.GetEnvironmentVariable("VEYRA_SMOKE_ROOT") ?? root, $"smoke-{smoke}.txt"), message);
        Environment.ExitCode = code;
        Close();
    }
}



























