// Golden-fixture parity tests for the C# checked decoders, mirroring
// bindings/typescript/tests/parity_cmis.test.mjs and bindings/go/parity_test.go:
// goldens are discovered per family under tests/golden (skipping `_`-prefixed
// files), fixtures are paired by stem, and the decoded result is serialized to
// a JSON tree and compared against the golden with numbers compared numerically
// and key order ignored. The root `_metadata` object is skipped everywhere, and
// `module.module_type` is skipped for CMIS (this binding emits null there; see
// the README parity section).
using System.Text.Json;
using Xunit;
using Xunit.Abstractions;

namespace OpenXcvr.Transceivers.Tests;

public class ParityTests
{
    private readonly ITestOutputHelper _output;

    public ParityTests(ITestOutputHelper output)
    {
        _output = output;
    }

    // ─── repo/golden discovery ──────────────────────────────────────────────

    private static readonly Lazy<string> RepoRootLazy = new(FindRepoRoot);

    /// <summary>Repo root discovered for golden/fixture lookup; shared by
    /// Sff8472PaddingTests to reach its committed Python-reference JSONs.</summary>
    internal static string RepoRoot => RepoRootLazy.Value;

    private static string FindRepoRoot()
    {
        var candidates = new List<string>();
        // OPENXCVR_REPO_ROOT wins over discovery (review follow-up A4), so
        // tests keep finding the goldens with out-of-tree artifact paths.
        var envRoot = Environment.GetEnvironmentVariable("OPENXCVR_REPO_ROOT");
        if (!string.IsNullOrEmpty(envRoot))
        {
            candidates.Add(envRoot);
        }
        for (var d = new DirectoryInfo(AppContext.BaseDirectory); d != null; d = d.Parent!)
        {
            candidates.Add(d.FullName);
        }
        var cwd = Directory.GetCurrentDirectory();
        candidates.Add(cwd);
        for (var d = new DirectoryInfo(cwd); d != null; d = d.Parent!)
        {
            candidates.Add(d.FullName);
        }
        foreach (var candidate in candidates)
        {
            if (Directory.Exists(Path.Combine(candidate, "tests", "golden", "cmis")))
            {
                return candidate;
            }
        }
        throw new InvalidOperationException(
            $"repo root not found from {AppContext.BaseDirectory} or {cwd}");
    }

    private static string GoldenDir(string family) =>
        Path.Combine(RepoRootLazy.Value, "tests", "golden", family);

    private static string FixtureDir(string family) =>
        Path.Combine(RepoRootLazy.Value, "tests", "fixtures", family);

    private static List<string> DiscoverGoldens(string family) =>
        Directory.EnumerateFiles(GoldenDir(family), "*.json")
            .Where(p => !Path.GetFileName(p).StartsWith('_'))
            .OrderBy(p => Path.GetFileName(p), StringComparer.Ordinal)
            .ToList();

    private static byte[] ReadFixture(string family, string file)
    {
        var path = Path.Combine(FixtureDir(family), file);
        if (!File.Exists(path))
        {
            throw new FileNotFoundException($"Required fixture missing for golden: {path}");
        }
        return File.ReadAllBytes(path);
    }

    // ─── comparator (numeric compare, key order ignored) ────────────────────

    internal static List<string> DeepCompare(
        JsonElement actual,
        JsonElement expected,
        string currentPath = "",
        bool skipModuleType = false)
    {
        if (actual.ValueKind == JsonValueKind.Number && expected.ValueKind == JsonValueKind.Number)
        {
            // Numbers compare numerically: 3, 3.0 and 3.00 are equal.
            if (actual.GetDecimal() != expected.GetDecimal())
            {
                return
                [
                    $"Mismatch at {PathOf(currentPath)}: expected {expected.GetRawText()}, got {actual.GetRawText()}",
                ];
            }
            return [];
        }

        if (actual.ValueKind != expected.ValueKind)
        {
            return
                [$"Mismatch at {PathOf(currentPath)}: kind expected {expected.ValueKind}, got {actual.ValueKind} (expected {expected.GetRawText()}, got {actual.GetRawText()})"];
        }

        switch (actual.ValueKind)
        {
            case JsonValueKind.Array:
                if (actual.GetArrayLength() != expected.GetArrayLength())
                {
                    return
                        [$"Mismatch at {PathOf(currentPath)}: length expected {expected.GetArrayLength()}, got {actual.GetArrayLength()}"];
                }
                var diffs = new List<string>();
                for (var i = 0; i < actual.GetArrayLength(); i++)
                {
                    diffs.AddRange(DeepCompare(actual[i], expected[i], $"{currentPath}[{i}]", false));
                }
                return diffs;

            case JsonValueKind.Object:
                var isRoot = currentPath == "";
                var isModule = currentPath == "module";

                bool FilterKey(string key) =>
                    !(isRoot && key == "_metadata") && !(isModule && skipModuleType && key == "module_type");

                var actualKeys = actual.EnumerateObject().Where(p => FilterKey(p.Name))
                    .Select(p => p.Name).OrderBy(k => k, StringComparer.Ordinal).ToList();
                var expectedKeys = expected.EnumerateObject().Where(p => FilterKey(p.Name))
                    .Select(p => p.Name).OrderBy(k => k, StringComparer.Ordinal).ToList();

                var objectDiffs = new List<string>();
                var missing = expectedKeys.Where(k => !actualKeys.Contains(k)).ToList();
                var unexpected = actualKeys.Where(k => !expectedKeys.Contains(k)).ToList();
                if (missing.Count > 0)
                {
                    objectDiffs.Add($"Missing keys at {PathOf(currentPath)}: {string.Join(", ", missing)}");
                }
                if (unexpected.Count > 0)
                {
                    objectDiffs.Add($"Unexpected keys at {PathOf(currentPath)}: {string.Join(", ", unexpected)}");
                }
                foreach (var key in actualKeys)
                {
                    if (expectedKeys.Contains(key))
                    {
                        objectDiffs.AddRange(DeepCompare(
                            actual.GetProperty(key), expected.GetProperty(key),
                            currentPath == "" ? key : $"{currentPath}.{key}", skipModuleType));
                    }
                }
                return objectDiffs;

            default:
                // Compare string values semantically: GetRawText would keep
                // JSON escape sequences (\u002B etc.) and produce false
                // mismatches between equal strings.
                var actualText = actual.ValueKind == JsonValueKind.String ? actual.GetString() : actual.GetRawText();
                var expectedText = expected.ValueKind == JsonValueKind.String ? expected.GetString() : expected.GetRawText();
                if (!string.Equals(actualText, expectedText, StringComparison.Ordinal))
                {
                    return
                        [$"Mismatch at {PathOf(currentPath)}: expected {expectedText}, got {actualText}"];
                }
                return [];
        }
    }

    private static string PathOf(string currentPath) => currentPath == "" ? "<root>" : currentPath;

    private void AssertParity(
        string family,
        Func<string, Dictionary<string, object?>> decode,
        bool skipModuleType)
    {
        var goldens = DiscoverGoldens(family);
        Assert.True(goldens.Count > 0, $"Discovered goldens must be > 0 for {family}");

        var executed = 0;
        foreach (var goldenPath in goldens)
        {
            var stem = Path.GetFileNameWithoutExtension(goldenPath);
            using var document = JsonDocument.Parse(File.ReadAllText(goldenPath));
            var expected = document.RootElement;

            var actualTree = decode(stem);
            executed++;

            var actualJson = JsonSerializer.Serialize(actualTree);
            using var actualDocument = JsonDocument.Parse(actualJson);
            var diffs = DeepCompare(actualDocument.RootElement, expected, "", skipModuleType);
            Assert.True(
                diffs.Count == 0,
                $"Parity mismatch in {Path.GetFileName(goldenPath)}:\n{string.Join("\n", diffs)}");
        }

        _output.WriteLine($"{family}: executed {executed} == discovered {goldens.Count}");
        Assert.Equal(goldens.Count, executed);
    }

    // ─── parity tests ───────────────────────────────────────────────────────

    [Fact]
    public void CmisGoldenParity()
    {
        AssertParity(
            "cmis",
            stem => Decoder.DecodeCmis(
                ReadFixture("cmis", $"{stem}.lower.bin"),
                ReadFixture("cmis", $"{stem}.page-00h.bin"),
                File.Exists(Path.Combine(FixtureDir("cmis"), $"{stem}.page-01h.bin"))
                    ? ReadFixture("cmis", $"{stem}.page-01h.bin")
                    : null,
                File.Exists(Path.Combine(FixtureDir("cmis"), $"{stem}.page-11h.bin"))
                    ? ReadFixture("cmis", $"{stem}.page-11h.bin")
                    : null,
                LoadVdmPages("cmis", stem)),
            skipModuleType: true);
    }

    // ─── focused latch pin ──────────────────────────────────────────────

    // Pin the per-lane status latches (rx_los ← LosFlagRx, Table 8-88;
    // tx_fault ← FailureFlagTx, Table 8-87) on the one fixture whose Page 11h
    // carries real latched values, independently of the deep golden compare:
    // fboss-cmis-4-0-qsfp112-200g-fr4 has rx_los latched on lanes 1/3 and
    // tx_fault on lanes 2/4, everything else clear.
    [Fact]
    public void CmisLaneStatusLatches()
    {
        const string stem = "fboss-cmis-4-0-qsfp112-200g-fr4";
        var result = Decoder.DecodeCmis(
            ReadFixture("cmis", $"{stem}.lower.bin"),
            ReadFixture("cmis", $"{stem}.page-00h.bin"),
            ReadFixture("cmis", $"{stem}.page-01h.bin"),
            ReadFixture("cmis", $"{stem}.page-11h.bin"),
            LoadVdmPages("cmis", stem));
        var lanes = Assert.IsType<List<object?>>(result["lanes"]);
        Assert.Equal(8, lanes.Count);
        for (var i = 0; i < 8; i++)
        {
            var idx = i + 1;
            var lane = Assert.IsType<Dictionary<string, object?>>(lanes[i]);
            Assert.Equal(idx == 1 || idx == 3, lane["rx_los"]);
            Assert.Equal(idx == 2 || idx == 4, lane["tx_fault"]);
        }
    }

    /// <summary>
    /// Loads a fixture's VDM page slices (CMIS § 8.27) keyed by tag
    /// '20h'..'2fh' for DecodeCmis; 2Eh is not part of the VDM feature and
    /// has no slice. The complete set gates the `vdm` block.
    /// </summary>
    private static Dictionary<string, byte[]> LoadVdmPages(string family, string stem)
    {
        var pages = new Dictionary<string, byte[]>();
        foreach (var tag in new[]
        {
            "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
            "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
        })
        {
            var path = Path.Combine(FixtureDir(family), $"{stem}.page-{tag}.bin");
            if (File.Exists(path))
            {
                pages[tag] = File.ReadAllBytes(path);
            }
        }
        return pages;
    }

    [Fact]
    public void Sff8636GoldenParity()
    {
        AssertParity(
            "sff-8636",
            stem => Decoder.DecodeSff8636(
                ReadFixture("sff-8636", $"{stem}.lower.bin"),
                ReadFixture("sff-8636", $"{stem}.page-00h.bin")),
            skipModuleType: false);
    }

    [Fact]
    public void Sff8472GoldenParity()
    {
        AssertParity(
            "sff-8472",
            stem => Decoder.DecodeSff8472(
                ReadFixture("sff-8472", $"{stem}.a0h.bin"),
                ReadFixture("sff-8472", $"{stem}.a2h.bin")),
            skipModuleType: false);
    }

    // ─── comparator self-test (mirrors the TS parity test) ─────────────────

    [Fact]
    public void ComparatorSelfTestDetectsMutation()
    {
        var goldens = DiscoverGoldens("cmis");
        Assert.True(goldens.Count > 0, "Must have at least one golden file for self-test");
        using var document = JsonDocument.Parse(File.ReadAllText(goldens[0]));
        var expected = document.RootElement.Clone();

        // Mutate a deep field and confirm the comparator reports it with its path.
        var mutatedNode = System.Text.Json.Nodes.JsonNode.Parse(expected.GetRawText())!;
        mutatedNode["identity"]!["vendor_name"] = "MUTATED_VENDOR_FOR_TEST";
        var mutatedJson = mutatedNode.ToJsonString();
        using var mutatedDocument = JsonDocument.Parse(mutatedJson);
        var diffs = DeepCompare(mutatedDocument.RootElement, expected, "", true);
        Assert.True(diffs.Count > 0, "Comparator self-test must detect mutation");
        Assert.Contains(diffs, d => d.Contains("identity.vendor_name"));

        // A numeric-only difference (66 vs 66.0) must NOT be reported.
        using var left = JsonDocument.Parse("{\"v\": 66}");
        using var right = JsonDocument.Parse("{\"v\": 66.0}");
        Assert.Empty(DeepCompare(left.RootElement, right.RootElement));
    }
}
