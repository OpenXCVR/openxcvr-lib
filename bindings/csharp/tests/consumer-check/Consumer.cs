// Installed-artifact consumer check (review follow-up A5): decodes one
// fixture per family using ONLY the packed OpenXcvr.Transceivers nupkg and
// exits nonzero unless every decode_status is "ok". Invoked by
// run-consumer-check.sh with the repo root as the only argument.
using OpenXcvr;

if (args.Length != 1)
{
    Console.Error.WriteLine("usage: Consumer <repo-root>");
    return 2;
}
var root = args[0];

var failures = new List<string>();

void Check(string family, string stem, Dictionary<string, object?>? tree)
{
    var status = tree?["decode_status"];
    Console.WriteLine($"{family}/{stem}: decode_status={status}");
    if (!Equals(status, "ok"))
    {
        failures.Add($"{family}/{stem}");
    }
}

// CMIS: first stem with lower + page-00h; optional pages when present.
var cmisDir = Path.Combine(root, "tests", "fixtures", "cmis");
var cmisStem = FirstStem(cmisDir, ".lower.bin", ".page-00h.bin");
Check("cmis", cmisStem, Decoder.DecodeCmis(
    File.ReadAllBytes(Path.Combine(cmisDir, cmisStem + ".lower.bin")),
    File.ReadAllBytes(Path.Combine(cmisDir, cmisStem + ".page-00h.bin")),
    Opt(cmisDir, cmisStem + ".page-01h.bin"),
    Opt(cmisDir, cmisStem + ".page-11h.bin")));

// SFF-8636: first stem with lower + page-00h.
var sff8636Dir = Path.Combine(root, "tests", "fixtures", "sff-8636");
var sff8636Stem = FirstStem(sff8636Dir, ".lower.bin", ".page-00h.bin");
Check("sff-8636", sff8636Stem, Decoder.DecodeSff8636(
    File.ReadAllBytes(Path.Combine(sff8636Dir, sff8636Stem + ".lower.bin")),
    File.ReadAllBytes(Path.Combine(sff8636Dir, sff8636Stem + ".page-00h.bin"))));

// SFF-8472: first stem with a0h + a2h.
var sff8472Dir = Path.Combine(root, "tests", "fixtures", "sff-8472");
var sff8472Stem = FirstStem(sff8472Dir, ".a0h.bin", ".a2h.bin");
Check("sff-8472", sff8472Stem, Decoder.DecodeSff8472(
    File.ReadAllBytes(Path.Combine(sff8472Dir, sff8472Stem + ".a0h.bin")),
    File.ReadAllBytes(Path.Combine(sff8472Dir, sff8472Stem + ".a2h.bin"))));

if (failures.Count > 0)
{
    Console.Error.WriteLine($"consumer check FAILED: {string.Join(", ", failures)}");
    return 1;
}
Console.WriteLine("consumer check ok: every decode_status is ok");
return 0;

static byte[]? Opt(string dir, string file)
{
    var path = Path.Combine(dir, file);
    return File.Exists(path) ? File.ReadAllBytes(path) : null;
}

static string FirstStem(string dir, params string[] suffixes)
{
    foreach (var file in Directory.EnumerateFiles(dir, "*" + suffixes[0])
                 .OrderBy(f => f, StringComparer.Ordinal))
    {
        var name = Path.GetFileName(file)!;
        if (!name.EndsWith(suffixes[0], StringComparison.Ordinal))
        {
            continue;
        }
        var stem = name[..^suffixes[0].Length];
        if (suffixes.Skip(1).All(x => File.Exists(Path.Combine(dir, stem + x))))
        {
            return stem;
        }
    }
    throw new InvalidOperationException(
        $"no fixture with {string.Join("+", suffixes)} in {dir}");
}
