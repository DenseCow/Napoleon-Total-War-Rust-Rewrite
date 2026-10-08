// 0-C round 11 (sandbox). Raw dword/byte dumps out of the program memory, plus summaries.
// Companion to F0cDecomp.java, which cannot express a strided field read.
// Usage (headless postScript args): <targets_file> <output_file>
//   bytes:<addr>:<n>                       hex + dword dump of n bytes
//   u32:<base>:<stride>:<count>:<off>      the u32 at base+i*stride+off for i in 0..count, with a
//                                           zero / non-zero summary
//   u32runs:<base>:<stride>:<count>:<off>  same, but the values as inclusive ranges
//   desc:<base>:<stride>:<count>            all six dwords of each strided descriptor, as one line each
//   names:<base>:<stride>:<count>:<off>     the u32 at +off read as a char* (if it points into memory)
//   ptr:<addr>:<count>                     count dwords at addr
//@category NapoleonRE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

public class F0cRaw extends GhidraScript {
    PrintWriter out;

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        List<String> lines = Files.readAllLines(Paths.get(args[0]));
        out = new PrintWriter(new FileWriter(args[1], false));
        out.println("// raw memory dump (NOT original source). Program: " + currentProgram.getName()
            + "  imagebase=" + currentProgram.getImageBase());
        for (String raw : lines) {
            String l = raw.trim();
            if (l.isEmpty() || l.startsWith("#")) continue;
            if (l.startsWith("==")) { out.println("\n//////////////////// " + l); continue; }
            try {
                String[] p = l.split(":");
                switch (p[0]) {
                    case "bytes" -> bytes(p[1], Integer.parseInt(p[2]));
                    case "u32" -> u32(p[1], Integer.parseInt(p[2]), Integer.parseInt(p[3]), Integer.parseInt(p[4]), false);
                    case "u32runs" -> u32(p[1], Integer.parseInt(p[2]), Integer.parseInt(p[3]), Integer.parseInt(p[4]), true);
                    case "desc" -> desc(p[1], Integer.parseInt(p[2]), Integer.parseInt(p[3]));
                    case "names" -> names(p[1], Integer.parseInt(p[2]), Integer.parseInt(p[3]), Integer.parseInt(p[4]));
                    case "ptr" -> ptr(p[1], Integer.parseInt(p[2]));
                    default -> out.println("// unknown target " + l);
                }
            } catch (Exception e) {
                out.println("// ERROR on '" + l + "': " + e);
            }
            out.flush();
        }
        out.close();
    }

    Address addr(String s) { return toAddr(Long.parseLong(s.replace("0x", ""), 16)); }

    long rd(Address a) throws Exception { return getInt(a) & 0xffffffffL; }

    boolean codeish(long v) throws Exception {
        Memory m = currentProgram.getMemory();
        try {
            Address a = toAddr(v);
            return m.contains(a) && m.getBlock(a).isExecute();
        } catch (Exception e) { return false; }
    }

    void bytes(String s, int n) throws Exception {
        Address base = addr(s);
        out.println("// bytes " + base + " .. " + base.add(n - 1L));
        Memory m = currentProgram.getMemory();
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < n; i++) {
            int b = m.getByte(base.add(i)) & 0xff;
            sb.append(String.format("%02x ", b));
            if (i % 32 == 31) { out.println("//  +" + String.format("%04x", i - 31) + "  " + sb); sb.setLength(0); }
        }
        if (sb.length() > 0) out.println("//            " + sb);
    }

    void ptr(String s, int count) throws Exception {
        Address base = addr(s);
        for (int i = 0; i < count; i++) {
            Address a = base.add(4L * i);
            long v = rd(a);
            out.println(String.format("ptr %s +0x%02x  dword[%d] = %08x%s", base, 4 * i, i, v,
                codeish(v) ? "  (code)" : ""));
        }
    }

    void u32(String s, int stride, int count, int off, boolean runs) throws Exception {
        Address base = addr(s);
        out.println(String.format("// u32 at %s stride 0x%x off 0x%x over %d entries", base, stride, off, count));
        int zero = 0, nonzero = 0;
        List<long[]> r = new ArrayList<>();
        int start = -1;
        long prev = Long.MIN_VALUE;
        StringBuilder line = new StringBuilder();
        for (int i = 0; i < count; i++) {
            long v = rd(base.add((long) stride * i + off));
            if (v == 0) zero++; else nonzero++;
            if (runs) {
                if (v != prev) {
                    if (start >= 0) r.add(new long[] { start, i - 1 });
                    start = i;
                    prev = v;
                }
            } else {
                if (line.length() > 0 && line.length() % 96 > 88) { out.println("//   " + line); line = new StringBuilder(); }
                line.append(i).append('=').append(v == 0 ? "0" : Long.toHexString(v)).append(' ');
            }
        }
        if (runs) { if (start >= 0) r.add(new long[] { start, count - 1 }); }
        if (line.length() > 0) out.println("//   " + line);
        if (runs) {
            StringBuilder sb = new StringBuilder();
            for (long[] g : r) {
                if (g[0] == g[1]) sb.append(g[0]);
                else if (g[1] - g[0] == 1) sb.append(g[0]).append(',').append(g[1]);
                else sb.append(g[0]).append("..").append(g[1]);
                sb.append(' ');
                if (sb.length() > 400) { out.println("//   zero-runs: " + sb); sb.setLength(0); }
            }
            if (sb.length() > 0) out.println("//   zero-runs: " + sb);
        }
        out.println(String.format("// SUMMARY entries=%d zero=%d nonzero=%d", count, zero, nonzero));
    }

    void desc(String s, int stride, int count) throws Exception {
        Address base = addr(s);
        int words = stride / 4;
        out.println(String.format("// descriptors at %s stride 0x%x, %d entries, %d dwords each", base, stride, count, words));
        for (int i = 0; i < count; i++) {
            StringBuilder sb = new StringBuilder();
            sb.append(i).append(':');
            for (int k = 0; k < words; k++) sb.append(' ').append(String.format("%08x", rd(base.add((long) stride * i + 4L * k))));
            out.println("//   " + sb);
        }
    }

    void names(String s, int stride, int count, int off) throws Exception {
        Address base = addr(s);
        out.println(String.format("// strings at %s stride 0x%x off 0x%x over %d entries", base, stride, off, count));
        for (int i = 0; i < count; i++) {
            Address a = base.add((long) stride * i + off);
            long v = rd(a);
            String sname = "?";
            try {
                Memory m = currentProgram.getMemory();
                Address t = toAddr(v);
                if (v != 0 && m.contains(t)) {
                    StringBuilder sb = new StringBuilder();
                    for (int k = 0; k < 96; k++) {
                        int b = m.getByte(t.add(k)) & 0xff;
                        if (b == 0) break;
                        sb.append((char) b);
                    }
                    sname = sb.toString();
                }
            } catch (Exception e) { }
            out.println(i + "\t" + Long.toHexString(v) + "\t" + sname);
        }
    }
}
