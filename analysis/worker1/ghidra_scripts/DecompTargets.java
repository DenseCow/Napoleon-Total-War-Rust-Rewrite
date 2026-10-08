// Decompile a targeted set of functions in Napoleon.exe and write Ghidra reconstructed pseudocode.
// Usage (headless postScript args): <targets_file> <output_file>
// targets file lines:
//   fn:0x00401000          decompile the function containing that address
//   ref:0x01300000         decompile every function referencing that address (e.g. a string VA)
//   str:needle             find defined strings containing needle, decompile referencing functions
//   callers:0x00401000     list callers of the function (no decompile) + decompile up to 3 callers
//   # comment / "== label" section header
// Output is labelled "Ghidra reconstructed pseudocode" - NOT original source.
//@category NapoleonRE
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.data.StringDataInstance;
import java.io.*;
import java.nio.file.*;
import java.util.*;

public class DecompTargets extends GhidraScript {
    DecompInterface di;
    PrintWriter out;
    Set<Address> done = new HashSet<>();
    int maxLines = 260;

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        List<String> lines = Files.readAllLines(Paths.get(args[0]));
        out = new PrintWriter(new FileWriter(args[1], false));
        if (args.length > 2) maxLines = Integer.parseInt(args[2]);
        out.println("// Ghidra reconstructed pseudocode (NOT original source). Program: " + currentProgram.getName());
        di = new DecompInterface();
        DecompileOptions opt = new DecompileOptions();
        di.setOptions(opt);
        di.openProgram(currentProgram);
        for (String raw : lines) {
            String l = raw.trim();
            if (l.isEmpty() || l.startsWith("#")) continue;
            if (l.startsWith("==")) { out.println("\n\n//////////////////// " + l); continue; }
            try {
                if (l.startsWith("fn:")) decomp(func(l.substring(3)), "fn " + l.substring(3));
                else if (l.startsWith("ref:")) {
                    Address a = addr(l.substring(4));
                    int n = 0;
                    for (Reference r : getReferencesTo(a)) {
                        Function f = getFunctionContaining(r.getFromAddress());
                        if (f != null && n++ < 6) decomp(f, "ref to " + a + " from " + r.getFromAddress());
                    }
                    if (n == 0) out.println("// no code refs to " + a);
                } else if (l.startsWith("str:")) {
                    String needle = l.substring(4);
                    int n = 0;
                    DataIterator it = currentProgram.getListing().getDefinedData(true);
                    while (it.hasNext() && !monitor.isCancelled()) {
                        Data d = it.next();
                        if (!d.hasStringValue()) continue;
                        Object v = d.getValue();
                        if (v == null || !v.toString().contains(needle)) continue;
                        for (Reference r : getReferencesTo(d.getAddress())) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            if (f != null && n++ < 4) decomp(f, "string '" + v.toString().replace("\n", " ") + "' @" + d.getAddress());
                        }
                        if (n >= 4) break;
                    }
                    if (n == 0) out.println("// no functions found for str:" + needle);
                } else if (l.startsWith("tree:")) {
                    String[] p = l.substring(5).split(":");
                    tree(func(p[0]), Integer.parseInt(p[1]), Integer.parseInt(p.length > 2 ? p[2] : "4000"));
                } else if (l.startsWith("callers:")) {
                    Function f = func(l.substring(8));
                    out.println("\n// callers of " + f.getName() + " @" + f.getEntryPoint());
                    int n = 0;
                    for (Function c : f.getCallingFunctions(monitor)) {
                        out.println("//   " + c.getName() + " @" + c.getEntryPoint() + " size=" + c.getBody().getNumAddresses());
                        if (n++ < 3) decomp(c, "caller of " + f.getEntryPoint());
                    }
                }
            } catch (Exception e) {
                out.println("// ERROR on '" + l + "': " + e);
            }
            out.flush();
        }
        out.close();
    }

    void tree(Function f, int depth, int maxSize) {
        if (f == null || depth < 0) return;
        if (f.getBody().getNumAddresses() > maxSize) { out.println("// (skip large " + f.getName() + " @" + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses() + ")"); return; }
        boolean fresh = !done.contains(f.getEntryPoint());
        decomp(f, "tree depth " + depth);
        if (!fresh) return;
        for (Function c : f.getCalledFunctions(monitor)) {
            if (c.isThunk() || c.isExternal()) continue;
            tree(c, depth - 1, maxSize);
        }
    }

    Address addr(String s) { return toAddr(Long.parseLong(s.replace("0x", ""), 16)); }

    Function func(String s) throws Exception {
        Address a = addr(s);
        Function f = getFunctionContaining(a);
        if (f == null) { f = createFunction(a, null); }
        if (f == null) throw new Exception("no function at " + s);
        return f;
    }

    void decomp(Function f, String why) {
        if (f == null) return;
        out.println("\n// ===== " + f.getName() + " @" + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses() + "  [" + why + "]");
        if (done.contains(f.getEntryPoint())) { out.println("// (already decompiled above)"); return; }
        done.add(f.getEntryPoint());
        // callers / callees summary
        StringBuilder sb = new StringBuilder("// callers: ");
        int n = 0;
        for (Function c : f.getCallingFunctions(monitor)) { if (n++ < 8) sb.append(c.getEntryPoint()).append(' '); }
        sb.append("(total ").append(n).append(")  callees: ");
        n = 0;
        for (Function c : f.getCalledFunctions(monitor)) { if (n++ < 12) sb.append(c.getName()).append(' '); }
        sb.append("(total ").append(n).append(")");
        out.println(sb);
        DecompileResults r = di.decompileFunction(f, 90, monitor);
        if (r == null || !r.decompileCompleted()) { out.println("// decompile failed: " + (r == null ? "null" : r.getErrorMessage())); return; }
        String c = r.getDecompiledFunction().getC();
        String[] ls = c.split("\n");
        int lim = Math.min(ls.length, maxLines);
        for (int i = 0; i < lim; i++) out.println(ls[i]);
        if (ls.length > lim) out.println("// ... truncated " + (ls.length - lim) + " more lines");
    }
}
