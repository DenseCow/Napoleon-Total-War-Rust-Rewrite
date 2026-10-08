// 0-C (middleware) worker copy of the audio worker copy of worker1 DecompTargets.java, plus "vt:<vtable>:<slots>".
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

public class F0cDecomp extends GhidraScript {
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
                } else if (l.startsWith("strat:")) {
                    // strat:a,b,c print the defined string at each address
                    for (String s : l.substring(6).split(",")) {
                        Address a = addr(s.trim());
                        Data d = getDataAt(a);
                        String v = (d != null && d.hasStringValue()) ? d.getValue().toString() : "(no string)";
                        out.println("// strat " + a + " '" + v + "'");
                    }
                } else if (l.startsWith("ptrs:")) {
                    // ptrs:addr,count print the string each u32 pointer of a table points to
                    String[] p = l.substring(5).split(",");
                    Address base = addr(p[0].trim());
                    int cnt = Integer.parseInt(p[1].trim());
                    for (int i = 0; i < cnt; i++) {
                        long v = getInt(base.add(4L * i)) & 0xffffffffL;
                        Data d = getDataAt(toAddr(v));
                        String s = (d != null && d.hasStringValue()) ? d.getValue().toString() : "(no string @" + Long.toHexString(v) + ")";
                        out.println("// ptrs " + i + " " + s);
                    }
                } else if (l.startsWith("strs:")) {
                    // strs:needle list every defined string containing needle, with the functions referencing it (no decompile)
                    String needle = l.substring(5);
                    DataIterator it = currentProgram.getListing().getDefinedData(true);
                    int n = 0;
                    while (it.hasNext() && !monitor.isCancelled()) {
                        Data d = it.next();
                        if (!d.hasStringValue()) continue;
                        Object v = d.getValue();
                        if (v == null || !v.toString().contains(needle)) continue;
                        StringBuilder sb = new StringBuilder("// strs " + d.getAddress() + " '" + v.toString().replace("\n", " ") + "' <-");
                        for (Reference r : getReferencesTo(d.getAddress())) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            sb.append(' ').append(f != null ? f.getEntryPoint().toString() : r.getFromAddress().toString() + "(nofn)");
                        }
                        out.println(sb);
                        if (++n > 600) break;
                    }
                } else if (l.startsWith("tree:")) {
                    String[] p = l.substring(5).split(":");
                    tree(func(p[0]), Integer.parseInt(p[1]), Integer.parseInt(p.length > 2 ? p[2] : "4000"));
                } else if (l.startsWith("vt:")) {
                    // vt:<vtable addr>:<slots e.g. 0,1,2> decompile the functions in those vtable slots
                    String[] p = l.substring(3).split(":");
                    Address base = addr(p[0]);
                    for (String s : p[1].split(",")) {
                        int k = Integer.parseInt(s.trim());
                        long target = getInt(base.add(4L * k)) & 0xffffffffL;
                        out.println("\n// vtable " + base + " slot " + k + " (+0x" + Integer.toHexString(4 * k) + ") -> " + Long.toHexString(target));
                        decomp(func(Long.toHexString(target)), "vt slot " + k);
                    }
                } else if (l.startsWith("xref:")) {
                    // xref:<addr> list every reference to the address (code or data)
                    Address a = addr(l.substring(5));
                    for (Reference r : getReferencesTo(a)) {
                        Function f = getFunctionContaining(r.getFromAddress());
                        out.println("// xref " + a + " <- " + r.getFromAddress() + " " + r.getReferenceType() + (f != null ? " in " + f.getName() : ""));
                    }
                } else if (l.startsWith("ptrs:")) {
                    // ptrs:<addr>:<count> dump consecutive dwords; show the string each points to (if any)
                    String[] p = l.substring(5).split(":");
                    Address base = addr(p[0]);
                    int cnt = Integer.parseInt(p[1]);
                    for (int k = 0; k < cnt; k++) {
                        long v = getInt(base.add(4L * k)) & 0xffffffffL;
                        String s = "";
                        try {
                            Data d = getDataAt(toAddr(v));
                            if (d != null && d.hasStringValue()) s = d.getValue().toString();
                        } catch (Exception e) { }
                        out.println(String.format("%d\t%s\t%08x\t%s", k, base.add(4L * k), v, s));
                    }
                } else if (l.startsWith("ext:")) {
                    // ext:<prefix> list external (imported) functions starting with prefix and every call site
                    String pre = l.substring(4);
                    SymbolIterator it = currentProgram.getSymbolTable().getExternalSymbols();
                    while (it.hasNext()) {
                        Symbol s = it.next();
                        if (!s.getName().contains(pre)) continue;
                        out.println("// import " + s.getName());
                        Set<Address> seen = new HashSet<>();
                        for (Reference r0 : s.getReferences()) {
                            // thunks / IAT slots: follow one level
                            List<Reference> rs = new ArrayList<>();
                            rs.add(r0);
                            for (Reference r1 : getReferencesTo(r0.getFromAddress())) rs.add(r1);
                            for (Reference r : rs) {
                                Function f = getFunctionContaining(r.getFromAddress());
                                if (f == null || !seen.add(r.getFromAddress())) continue;
                                out.println("//   " + r.getFromAddress() + " " + r.getReferenceType() + " in " + f.getName() + " @" + f.getEntryPoint());
                            }
                        }
                    }
                } else if (l.startsWith("vcall:")) {
                    // vcall:<disp hex>[:<context lines>] list CALL [reg + disp] sites (virtual calls through a vtable slot),
                    // with the previous few instructions (to see pushed constants)
                    String[] p = l.substring(6).split(":");
                    String needle = "+ 0x" + p[0].toLowerCase().replace("0x", "") + "]";
                    int ctx = p.length > 1 ? Integer.parseInt(p[1]) : 0;
                    InstructionIterator ii = currentProgram.getListing().getInstructions(true);
                    int n = 0;
                    while (ii.hasNext() && !monitor.isCancelled()) {
                        Instruction ins = ii.next();
                        if (!ins.getMnemonicString().equals("CALL")) continue;
                        String s = ins.toString().toLowerCase();
                        if (!s.contains(needle)) continue;
                        Function f = getFunctionContaining(ins.getAddress());
                        out.println("// vcall " + ins.getAddress() + " " + s + " in " + (f != null ? f.getName() : "?"));
                        Instruction pr = ins.getPrevious();
                        for (int k = 0; k < ctx && pr != null; k++, pr = pr.getPrevious()) out.println("//      " + pr.getAddress() + " " + pr);
                        if (++n > 3000) break;
                    }
                } else if (l.startsWith("dis:")) {
                    // dis:<addr>:<count> list instructions starting at addr
                    String[] p = l.substring(4).split(":");
                    Instruction ins = getInstructionAt(addr(p[0]));
                    if (ins == null) ins = getInstructionAfter(addr(p[0]));
                    out.println("\n// dis " + p[0]);
                    for (int k = 0; k < Integer.parseInt(p[1]) && ins != null; k++, ins = ins.getNext())
                        out.println("//   " + ins.getAddress() + " " + ins);
                } else if (l.startsWith("insnr:")) {
                    // insnr:<start>:<end>:<substring> instructions in an address range whose text contains the substring
                    String[] p = l.substring(6).split(":", 3);
                    Address lo = addr(p[0]), hi = addr(p[1]);
                    String needle = p[2].toLowerCase();
                    InstructionIterator ii = currentProgram.getListing().getInstructions(lo, true);
                    int n = 0;
                    while (ii.hasNext() && !monitor.isCancelled()) {
                        Instruction ins = ii.next();
                        if (ins.getAddress().compareTo(hi) > 0) break;
                        String s = ins.toString().toLowerCase();
                        if (!s.contains(needle)) continue;
                        Function f = getFunctionContaining(ins.getAddress());
                        out.println("// insn " + ins.getAddress() + " " + s + " in " + (f != null ? f.getName() : "?"));
                        if (++n > 400000) break;
                    }
                } else if (l.startsWith("insn:")) {
                    // insn:<substring> list instructions whose text contains the substring (e.g. a struct offset "+ 0xc16c]")
                    String needle = l.substring(5).toLowerCase();
                    InstructionIterator ii = currentProgram.getListing().getInstructions(true);
                    int n = 0;
                    while (ii.hasNext() && !monitor.isCancelled()) {
                        Instruction ins = ii.next();
                        String s = ins.toString().toLowerCase();
                        if (!s.contains(needle)) continue;
                        Function f = getFunctionContaining(ins.getAddress());
                        out.println("// insn " + ins.getAddress() + " " + s + " in " + (f != null ? f.getName() : "?"));
                        if (++n > 400) break;
                    }
                } else if (l.startsWith("exported:")) {
                    // exported:<name> decompile a function by symbol name (e.g. an export of mss32.dll)
                    String nm = l.substring(9);
                    boolean any = false;
                    for (Symbol s : currentProgram.getSymbolTable().getSymbols(nm)) {
                        Function f = getFunctionAt(s.getAddress());
                        if (f != null) { decomp(f, "symbol " + nm); any = true; }
                    }
                    if (!any) out.println("// no function symbol " + nm);
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
