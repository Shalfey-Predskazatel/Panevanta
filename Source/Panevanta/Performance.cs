using System.Diagnostics;
using System.Globalization;
using System.Text.RegularExpressions;

namespace Veyra;

internal sealed partial class Performance
{
    private long previousFrames, previousQpc;
    private readonly Rate capture = new();
    private readonly Rate present = new();
    public double? PresentFps { get; private set; }
    public double? CaptureFps { get; private set; }
    public double? Age { get; private set; }
    public int Drops { get; private set; }
    public long Samples { get; private set; }
    public double MeanPresent { get; private set; }
    public double MeanAge { get; private set; }
    public int AgeSamples { get; private set; }
    public int Backwards { get; private set; }
    private double previousPts = -1;
    private int lateSamples;

    public bool NeedsReset(int input) => CaptureFps >= input * 0.5 && previousQpc != 0 &&
        (lateSamples >= 3 || Stopwatch.GetElapsedTime(previousQpc).TotalSeconds > 2);

    [GeneratedRegex(@"PANE_RENDER (\d+) (\d+) ([0-9.]+) (\d+)", RegexOptions.CultureInvariant)]
    private static partial Regex RenderLine();

    public void Read(Pipeline pipeline, int input)
    {
        long capturedAt = Stopwatch.GetTimestamp();
        CaptureFps = capture.Update(capturedAt, pipeline.Captured);
        using var file = new FileStream(pipeline.Log, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        file.Position = Math.Max(0, file.Length - 64 * 1024);
        using var reader = new StreamReader(file);
        var matches = RenderLine().Matches(reader.ReadToEnd());
        if (matches.Count == 0) return;
        var fields = matches[^1].Groups;
        var frames = long.Parse(fields[1].Value, CultureInfo.InvariantCulture);
        var now = long.Parse(fields[2].Value, CultureInfo.InvariantCulture);
        var pts = double.Parse(fields[3].Value, CultureInfo.InvariantCulture);
        // A stopped renderer leaves its last log line intact.
        bool current = capturedAt - now <= Stopwatch.Frequency * 2;
        PresentFps = current ? present.Update(now, frames) : null;
        if (!current) Age = null;
        if (previousQpc != 0 && now > previousQpc && frames >= previousFrames)
        {
            double seconds = (now - previousQpc) / (double)Stopwatch.Frequency;
            if (pts < previousPts - 0.0001) Backwards++;
            Samples++;
            MeanPresent += ((frames - previousFrames) / seconds - MeanPresent) / Samples;
            long sequence = 1 + (long)Math.Floor(pts * input + 0.001);
            long timestamp = pipeline.CaptureTime(sequence);
            if (timestamp == 0) timestamp = pipeline.CaptureTime(sequence - 1);
            double age = (now - timestamp) * 1000.0 / Stopwatch.Frequency;
            lateSamples = timestamp != 0 && age >= 250 && CaptureFps >= input * 0.5 ? lateSamples + 1 : 0;
            Age = timestamp != 0 && age is >= 0 and <= 1500 ? age : null;
            if (Age is not null)
            {
                AgeSamples++;
                MeanAge += (age - MeanAge) / AgeSamples;
            }
        }
        if (now != previousQpc)
        {
            previousPts = pts;
            previousQpc = now;
            previousFrames = frames;
            Drops = int.Parse(fields[4].Value, CultureInfo.InvariantCulture);
        }
    }

    private sealed class Rate
    {
        private readonly Queue<(long Time, long Frames)> samples = new();
        private long lastSample;
        private double? fps;

        public double? Update(long time, long frames)
        {
            if (time <= lastSample) return fps;
            lastSample = time;
            samples.Enqueue((time, frames));
            long cutoff = time - Stopwatch.Frequency * 2;
            while (samples.Count > 1 && samples.Peek().Time < cutoff) samples.Dequeue();
            var oldest = samples.Peek();
            if (time - oldest.Time < Stopwatch.Frequency) return fps = null;
            return fps = (frames - oldest.Frames) * (double)Stopwatch.Frequency / (time - oldest.Time);
        }
    }
}

