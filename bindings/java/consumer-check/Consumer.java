package com.openxcvr.consumer;

import com.openxcvr.Openxcvr;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Map;

/**
 * Installed-artifact consumer (review follow-up A4, design D3).
 *
 * <p>Compiled with {@code javac --release 11} against ONLY the built JAR — no
 * test-scope dependencies, no repo sources — so it proves the published
 * artifact is self-contained: release-11 bytecode, zero runtime deps, and a
 * usable facade.
 *
 * <p>Args, in order: CMIS lower, page-00h, page-01h, page-11h; SFF-8636 lower,
 * page-00h; SFF-8472 A0h, A2h. Exits nonzero unless every family decodes with
 * {@code decode_status == "ok"}.
 */
public final class Consumer {

    private Consumer() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 8) {
            System.err.println("usage: Consumer <cmis.lower> <cmis.00h> <cmis.01h> <cmis.11h>"
                    + " <sff8636.lower> <sff8636.00h> <sff8472.a0h> <sff8472.a2h>");
            System.exit(2);
        }

        int failures = 0;

        Map<String, Object> cmis = Openxcvr.decodeCmis(
                read(args[0]), read(args[1]), read(args[2]), read(args[3]));
        failures += check("cmis", cmis);

        Map<String, Object> sff8636 =
                Openxcvr.decodeSff8636(read(args[4]), read(args[5]));
        failures += check("sff-8636", sff8636);

        Map<String, Object> sff8472 =
                Openxcvr.decodeSff8472(read(args[6]), read(args[7]));
        failures += check("sff-8472", sff8472);

        if (failures != 0) {
            System.exit(1);
        }
        System.out.println("consumer check: 3/3 families decode_status=ok");
    }

    private static int check(String family, Map<String, Object> result) {
        Object status = result.get("decode_status");
        if (!"ok".equals(status)) {
            System.err.println("consumer check: " + family + " decode_status=" + status);
            return 1;
        }
        System.out.println("consumer check: " + family + " decode_status=ok");
        return 0;
    }

    private static byte[] read(String path) throws Exception {
        return Files.readAllBytes(Paths.get(path));
    }
}
