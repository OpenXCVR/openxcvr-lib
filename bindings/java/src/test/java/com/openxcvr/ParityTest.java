package com.openxcvr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.openxcvr.generated.CmisMediaInterface;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;
import java.util.stream.Collectors;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;

/**
 * Golden parity for the checked decoders, mirroring the TypeScript
 * {@code parity_cmis.test.mjs} harness: goldens are discovered per family
 * under {@code tests/golden}, fixtures are paired by stem, and the decoded
 * result is compared to the golden as a JSON tree — numbers numerically,
 * key order ignored, skipping the root {@code _metadata} key and, for CMIS
 * only, {@code module.module_type} (this binding emits null there; SFF
 * module_type is compared).
 */
class ParityTest {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    /** Family decoder invoked as (primary, secondary) page pair. */
    @FunctionalInterface
    private interface PairDecoder {
        Map<String, Object> decode(byte[] primary, byte[] secondary);
    }

    // ─── repo layout helpers ────────────────────────────────────────────────

    private static Path repoRoot() {
        Path dir = Paths.get(System.getProperty("user.dir")).toAbsolutePath();
        for (int i = 0; i < 4; i++) {
            if (Files.isDirectory(dir.resolve("tests/golden"))) {
                return dir;
            }
            dir = dir.getParent();
        }
        throw new IllegalStateException(
                "repo root (tests/golden) not found from " + System.getProperty("user.dir"));
    }

    private static List<String> discoverGoldens(String family) {
        Path goldenDir = repoRoot().resolve("tests/golden").resolve(family);
        try (Stream<Path> entries = Files.list(goldenDir)) {
            return entries.map(p -> p.getFileName().toString())
                    .filter(f -> f.endsWith(".json") && !f.startsWith("_"))
                    .sorted()
                    .collect(Collectors.toList());
        } catch (IOException e) {
            throw new UncheckedIOException("cannot list " + goldenDir, e);
        }
    }

    private static byte[] readRequired(Path fixtureDir, String name, String golden) {
        Path p = fixtureDir.resolve(name);
        if (!Files.isRegularFile(p)) {
            throw new IllegalStateException("Required fixture missing for golden " + golden + ": " + p);
        }
        try {
            return Files.readAllBytes(p);
        } catch (IOException e) {
            throw new UncheckedIOException("cannot read " + p, e);
        }
    }

    private static byte[] readOptional(Path fixtureDir, String name) {
        Path p = fixtureDir.resolve(name);
        if (!Files.isRegularFile(p)) {
            return null;
        }
        try {
            return Files.readAllBytes(p);
        } catch (IOException e) {
            throw new UncheckedIOException("cannot read " + p, e);
        }
    }

    /** VDM page tags (CMIS § 8.27); 2Eh is not part of the VDM feature and
     * has no slice. */
    private static final String[] VDM_PAGE_TAGS = {
        "20h", "21h", "22h", "23h", "24h", "25h", "26h", "27h",
        "28h", "29h", "2ah", "2bh", "2ch", "2dh", "2fh",
    };

    /** Loads a fixture's VDM page slices keyed by tag; empty map when the
     * fixture carries none. */
    private static Map<String, byte[]> loadVdmPages(Path fixtureDir, String stem) {
        Map<String, byte[]> pages = new LinkedHashMap<>();
        for (String tag : VDM_PAGE_TAGS) {
            byte[] buf = readOptional(fixtureDir, stem + ".page-" + tag + ".bin");
            if (buf != null) {
                pages.put(tag, buf);
            }
        }
        return pages;
    }

    // ─── comparison ─────────────────────────────────────────────────────────

    private static List<String> deepCompare(
            JsonNode actual, JsonNode expected, String path, boolean skipModuleType) {
        List<String> diffs = new ArrayList<>();
        String label = path.isEmpty() ? "<root>" : path;

        if (expected.isNumber() || actual.isNumber()) {
            if (!actual.isNumber()
                    || !expected.isNumber()
                    || actual.decimalValue().compareTo(expected.decimalValue()) != 0) {
                diffs.add("Mismatch at " + label + ": expected " + expected + ", got " + actual);
            }
            return diffs;
        }

        if (actual.getNodeType() != expected.getNodeType()) {
            diffs.add("Mismatch at " + label + ": expected " + expected.getNodeType()
                    + " (" + expected + "), got " + actual.getNodeType() + " (" + actual + ")");
            return diffs;
        }

        switch (actual.getNodeType()) {
            case OBJECT:
                {
                    TreeSet<String> expectedKeys = new TreeSet<>();
                    expected.fieldNames().forEachRemaining(expectedKeys::add);
                    TreeSet<String> actualKeys = new TreeSet<>();
                    actual.fieldNames().forEachRemaining(actualKeys::add);
                    if (path.isEmpty()) {
                        expectedKeys.remove("_metadata");
                        actualKeys.remove("_metadata");
                    }
                    if (skipModuleType && path.equals("module")) {
                        expectedKeys.remove("module_type");
                        actualKeys.remove("module_type");
                    }
                    List<String> missing =
                            expectedKeys.stream()
                                    .filter(k -> !actualKeys.contains(k))
                                    .collect(Collectors.toList());
                    if (!missing.isEmpty()) {
                        diffs.add("Missing keys at " + label + ": " + String.join(", ", missing));
                    }
                    List<String> unexpected =
                            actualKeys.stream()
                                    .filter(k -> !expectedKeys.contains(k))
                                    .collect(Collectors.toList());
                    if (!unexpected.isEmpty()) {
                        diffs.add("Unexpected keys at " + label + ": " + String.join(", ", unexpected));
                    }
                    for (String key : expectedKeys) {
                        if (actualKeys.contains(key)) {
                            String childPath = path.isEmpty() ? key : path + "." + key;
                            diffs.addAll(
                                    deepCompare(
                                            actual.get(key),
                                            expected.get(key),
                                            childPath,
                                            skipModuleType));
                        }
                    }
                    return diffs;
                }
            case ARRAY:
                {
                    if (actual.size() != expected.size()) {
                        diffs.add("Mismatch at " + label + ": length expected " + expected.size()
                                + ", got " + actual.size());
                        return diffs;
                    }
                    for (int i = 0; i < actual.size(); i++) {
                        diffs.addAll(
                                deepCompare(
                                        actual.get(i), expected.get(i), path + "[" + i + "]",
                                        skipModuleType));
                    }
                    return diffs;
                }
            default:
                if (!actual.equals(expected)) {
                    diffs.add("Mismatch at " + label + ": expected " + expected + ", got " + actual);
                }
                return diffs;
        }
    }

    // ─── tests ──────────────────────────────────────────────────────────────

    @Test
    void comparatorSelfTestDetectsMutationWithPathLabel() throws IOException {
        List<String> goldens = discoverGoldens("cmis");
        assertFalse(goldens.isEmpty(), "Must have at least one golden file for self-test");

        JsonNode sample = null;
        for (String f : goldens) {
            JsonNode candidate =
                    MAPPER.readTree(repoRoot().resolve("tests/golden/cmis").resolve(f).toFile());
            if ("ok".equals(candidate.path("decode_status").asText())
                    && candidate.path("identity").isObject()) {
                sample = candidate;
                break;
            }
        }
        if (sample != null) {
            ObjectNode mutated = (ObjectNode) sample.deepCopy();
            ((ObjectNode) mutated.get("identity")).put("vendor_name", "MUTATED_VENDOR_FOR_TEST");
            List<String> diffs = deepCompare(mutated, sample, "", true);
            assertFalse(diffs.isEmpty(), "Comparator self-test must detect mutation");
            assertTrue(
                    diffs.stream().anyMatch(d -> d.contains("identity.vendor_name")),
                    "Expected diff mentioning identity.vendor_name, got: " + String.join("; ", diffs));
        } else {
            // No ok golden with an identity block: prove the comparator still
            // detects an injected extra key.
            String f = goldens.get(0);
            JsonNode candidate =
                    MAPPER.readTree(repoRoot().resolve("tests/golden/cmis").resolve(f).toFile());
            ObjectNode mutated = (ObjectNode) candidate.deepCopy();
            mutated.put("__extra_test_key", 123);
            List<String> diffs = deepCompare(mutated, candidate, "", true);
            assertFalse(diffs.isEmpty(), "Comparator self-test must detect mutation");
        }
    }

    @Test
    void cmisGoldenParity() throws IOException {
        Path fixtureDir = repoRoot().resolve("tests/fixtures/cmis");
        List<String> goldens = discoverGoldens("cmis");
        assertTrue(goldens.size() > 0, "Discovered CMIS goldens must be > 0");

        int executed = 0;
        for (String f : goldens) {
            String stem = f.substring(0, f.length() - ".json".length());
            JsonNode expected =
                    MAPPER.readTree(repoRoot().resolve("tests/golden/cmis").resolve(f).toFile());

            byte[] lower = readRequired(fixtureDir, stem + ".lower.bin", f);
            byte[] page00h = readRequired(fixtureDir, stem + ".page-00h.bin", f);
            byte[] page01h = readOptional(fixtureDir, stem + ".page-01h.bin");
            byte[] page11h = readOptional(fixtureDir, stem + ".page-11h.bin");

            // VDM pages (CMIS § 8.27), loaded when the fixture carries them:
            // the complete set (2Eh excluded) gates the `vdm` block in
            // decodeCmis.
            Map<String, byte[]> vdmPages = loadVdmPages(fixtureDir, stem);

            Map<String, Object> result =
                    Openxcvr.decodeCmis(lower, page00h, page01h, page11h, vdmPages);
            executed++;

            if ("ok".equals(result.get("decode_status"))) {
                Object module = result.get("module");
                assertTrue(module instanceof Map, "ok result must carry a module map in " + f);
                assertNull(
                        ((Map<?, ?>) module).get("module_type"),
                        "module.module_type must be null for ok result in " + f);
            }

            JsonNode actual = MAPPER.valueToTree(result);
            List<String> diffs = deepCompare(actual, expected, "", true);
            assertTrue(
                    diffs.isEmpty(),
                    "Parity mismatch in " + f + ":\n" + String.join("\n", diffs));
        }

        assertEquals(goldens.size(), executed, "Executed count must equal discovered count");
        System.out.println("cmis parity: executed " + executed + " of " + goldens.size()
                + " discovered goldens");
    }

    @Test
    void cmisLaneStatusLatches() throws IOException {
        // Pin the per-lane status latches (rx_los ← LOSFlagRx, Table 8-88;
        // tx_fault ← FailureFlagTx, Table 8-87) on the one fixture whose
        // Page 11h carries real latched values, independently of the deep
        // golden compare: fboss-cmis-4-0-qsfp112-200g-fr4 has rx_los latched
        // on lanes 1/3 and tx_fault on lanes 2/4, everything else clear.
        Path fixtureDir = repoRoot().resolve("tests/fixtures/cmis");
        String stem = "fboss-cmis-4-0-qsfp112-200g-fr4";
        Map<String, Object> result = Openxcvr.decodeCmis(
                readRequired(fixtureDir, stem + ".lower.bin", stem),
                readRequired(fixtureDir, stem + ".page-00h.bin", stem),
                readRequired(fixtureDir, stem + ".page-01h.bin", stem),
                readRequired(fixtureDir, stem + ".page-11h.bin", stem),
                loadVdmPages(fixtureDir, stem));
        Object lanesObj = result.get("lanes");
        assertTrue(lanesObj instanceof List, "lanes must be a list");
        List<?> lanes = (List<?>) lanesObj;
        assertEquals(8, lanes.size());
        for (int i = 0; i < 8; i++) {
            int idx = i + 1;
            @SuppressWarnings("unchecked")
            Map<String, Object> lane = (Map<String, Object>) lanes.get(i);
            assertEquals(Boolean.valueOf(idx == 1 || idx == 3), lane.get("rx_los"),
                    "lane " + idx + " rx_los");
            assertEquals(Boolean.valueOf(idx == 2 || idx == 4), lane.get("tx_fault"),
                    "lane " + idx + " tx_fault");
        }
    }

    @Test
    void cmisGoldenParityCoversOptionalPages() {
        Path fixtureDir = repoRoot().resolve("tests/fixtures/cmis");
        List<String> goldens = discoverGoldens("cmis");

        int page01h = 0;
        int page11h = 0;
        for (String f : goldens) {
            String stem = f.substring(0, f.length() - ".json".length());
            if (Files.isRegularFile(fixtureDir.resolve(stem + ".page-01h.bin"))) {
                page01h++;
            }
            if (Files.isRegularFile(fixtureDir.resolve(stem + ".page-11h.bin"))) {
                page11h++;
            }
        }

        assertTrue(page01h > 0, "At least one CMIS golden must supply page-01h, found " + page01h
                + " of " + goldens.size() + " goldens");
        assertTrue(page11h > 0, "At least one CMIS golden must supply page-11h, found " + page11h
                + " of " + goldens.size() + " goldens");
    }

    @Test
    void cmisMediaInterfaceNameRejectsOutOfDomainArguments() {
        assertNull(CmisMediaInterface.name(-1, 1), "mediaType below 0 must not name");
        assertNull(CmisMediaInterface.name(256, 1), "mediaType above 255 must not name");
        assertNull(CmisMediaInterface.name(1, -1), "code below 0 must not name");
        assertNull(CmisMediaInterface.name(1, 256), "code above 255 must not name");
    }

    @Test
    void sff8636GoldenParity() throws IOException {
        runSffParity(
                "sff-8636",
                ".lower.bin",
                ".page-00h.bin",
                Openxcvr::decodeSff8636);
    }

    @Test
    void sff8472GoldenParity() throws IOException {
        runSffParity(
                "sff-8472",
                ".a0h.bin",
                ".a2h.bin",
                Openxcvr::decodeSff8472);
    }

    private void runSffParity(String family, String primarySuffix, String secondarySuffix,
            PairDecoder decoder) throws IOException {
        Path fixtureDir = repoRoot().resolve("tests/fixtures").resolve(family);
        List<String> goldens = discoverGoldens(family);
        assertTrue(goldens.size() > 0, "Discovered " + family + " goldens must be > 0");

        int executed = 0;
        for (String f : goldens) {
            String stem = f.substring(0, f.length() - ".json".length());
            JsonNode expected =
                    MAPPER.readTree(
                            repoRoot().resolve("tests/golden").resolve(family).resolve(f).toFile());

            byte[] primary = readRequired(fixtureDir, stem + primarySuffix, f);
            byte[] secondary = readRequired(fixtureDir, stem + secondarySuffix, f);

            Map<String, Object> result = decoder.decode(primary, secondary);
            executed++;

            JsonNode actual = MAPPER.valueToTree(result);
            List<String> diffs = deepCompare(actual, expected, "", false);
            assertTrue(
                    diffs.isEmpty(),
                    "Parity mismatch in " + f + ":\n" + String.join("\n", diffs));
        }

        assertEquals(goldens.size(), executed, "Executed count must equal discovered count");
        System.out.println(family + " parity: executed " + executed + " of " + goldens.size()
                + " discovered goldens");
    }
}
