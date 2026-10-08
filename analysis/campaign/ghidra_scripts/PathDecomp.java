// Shaders worker (copy of analysis/speedtree SptDecomp): vtable, data and xref commands.
//   vt:0xFUNC      find data refs to FUNC (vtable slots); print each vtable (start found by walking
//                  back over function pointers) with slot indices
//   vtd:0xFUNC     like vt:, and decompile every slot function
//   dw:0xADDR:N    print N dwords (hex, int, float) at ADDR
//   xref:0xADDR    list every reference to ADDR (from address, type, containing function)
//   scal:0xV:0xLO:0xHI[:dec]  instructions in LO..HI using scalar V (field offsets); :dec decompiles
//   lst:0xLO:0xHI  disassembly listing
//   vtat:0xVT:N[:MIN]  list N slots of the vtable at VT, decompiling slot functions of size >= MIN
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
import ghidra.program.model.mem.MemoryBlock;
import java.io.*;
import java.nio.file.*;
import java.util.*;

public class PathDecomp extends GhidraScript {
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
                } else if (l.startsWith("bytes:") || l.startsWith("wbytes:")) {
                    // bytes:needle - find the ASCII bytes anywhere in memory, decompile the functions
                    // referencing each hit (or an instruction whose scalar equals the hit address);
                    // wbytes:needle does the same for the UTF-16LE spelling
                    boolean wide = l.startsWith("wbytes:");
                    String needle = l.substring(wide ? 7 : 6);
                    byte[] nb = needle.getBytes(wide ? "UTF-16LE" : "US-ASCII");
                    if (needle.startsWith("0x")) { // bytes:0x<hex> = raw bytes (e.g. a float, little-endian)
                        String h = needle.substring(2);
                        nb = new byte[h.length() / 2];
                        for (int i = 0; i < nb.length; i++) nb[i] = (byte) Integer.parseInt(h.substring(2 * i, 2 * i + 2), 16);
                    }
                    Address a = currentProgram.getMinAddress();
                    int hits = 0;
                    while (hits < 4) {
                        a = currentProgram.getMemory().findBytes(a, nb, null, true, monitor);
                        if (a == null) break;
                        hits++;
                        out.println("// bytes '" + needle + "' at " + a);
                        int n = 0;
                        for (Reference r : getReferencesTo(a)) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            out.println("//   ref from " + r.getFromAddress() + " in " + (f == null ? "-" : f.getName()));
                            if (f != null && n++ < 4) decomp(f, "bytes '" + needle + "'");
                        }
                        if (n == 0) {
                            long want = a.getOffset();
                            InstructionIterator it = currentProgram.getListing().getInstructions(true);
                            while (it.hasNext() && n < 4) {
                                Instruction ins = it.next();
                                for (int i = 0; i < ins.getNumOperands(); i++)
                                    for (Object o : ins.getOpObjects(i))
                                        if (o instanceof ghidra.program.model.scalar.Scalar && ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue() == want) {
                                            Function f = getFunctionContaining(ins.getAddress());
                                            out.println("//   scalar use at " + ins.getAddress() + " in " + (f == null ? "-" : f.getName()));
                                            if (f != null && n++ < 4) decomp(f, "scalar of bytes '" + needle + "'");
                                        }
                            }
                        }
                        a = a.add(1);
                    }
                } else if (l.startsWith("commit:")) {
                    // commit:0xLO:0xHI:passes - for every function in [LO, HI]: switch to __thiscall
                    // when the decompile reads in_ECX, then commit the decompiler's parameters and
                    // return type to the database (run without -readOnly to keep them)
                    String[] p = l.substring(7).split(":");
                    AddressSet range = new AddressSet(addr(p[0]), addr(p[1]));
                    int passes = p.length > 2 ? Integer.parseInt(p[2]) : 2;
                    for (int pass = 0; pass < passes; pass++) {
                        int n = 0, tc = 0;
                        for (Function f : currentProgram.getFunctionManager().getFunctions(range, true)) {
                            if (monitor.isCancelled()) break;
                            DecompileResults r = di.decompileFunction(f, 60, monitor);
                            if (r == null || !r.decompileCompleted()) continue;
                            String c = r.getDecompiledFunction().getC();
                            if (c.contains("in_ECX") && !"__thiscall".equals(f.getCallingConventionName())) {
                                f.setCallingConvention("__thiscall");
                                tc++;
                                r = di.decompileFunction(f, 60, monitor);
                                if (r == null || !r.decompileCompleted()) continue;
                            }
                            try {
                                ghidra.program.model.pcode.HighFunctionDBUtil.commitParamsToDatabase(r.getHighFunction(), true,
                                    ghidra.program.model.pcode.HighFunctionDBUtil.ReturnCommitOption.COMMIT, SourceType.ANALYSIS);
                                n++;
                            } catch (Exception e) { }
                        }
                        out.println("// commit pass " + pass + ": " + n + " functions, " + tc + " made __thiscall");
                        out.flush();
                    }
                } else if (l.startsWith("tc:")) {
                    // tc:0xF1,0xF2,... - mark these functions __thiscall in memory (not saved with
                    // -readOnly) so later decompiles show the object each call works on
                    for (String s : l.substring(3).split(",")) {
                        Function f = func(s.trim());
                        if (!"__thiscall".equals(f.getCallingConventionName())) f.setCallingConvention("__thiscall");
                    }
                    out.println("// thiscall set: " + l.substring(3));
                } else if (l.startsWith("funcs:")) {
                    // funcs:0xLO:0xHI - list functions in the range with size and caller count
                    String[] p = l.substring(6).split(":");
                    for (Function f : currentProgram.getFunctionManager().getFunctions(new AddressSet(addr(p[0]), addr(p[1])), true)) {
                        int nc = f.getCallingFunctions(monitor).size();
                        out.println("// " + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses() + " callers=" + nc + " " + f.getName());
                    }
                } else if (l.startsWith("ctree:")) {
                    // ctree:0xFUNC:depth - print the call tree (names, sizes, string refs), no decompile
                    String[] p = l.substring(6).split(":");
                    out.println("\n// call tree of " + p[0]);
                    ctree(func(p[0]), Integer.parseInt(p[1]), "", new HashSet<>());
                } else if (l.startsWith("tree:")) {
                    String[] p = l.substring(5).split(":");
                    tree(func(p[0]), Integer.parseInt(p[1]), Integer.parseInt(p.length > 2 ? p[2] : "4000"));
                } else if (l.startsWith("vt:") || l.startsWith("vtd:")) {
                    boolean dec = l.startsWith("vtd:");
                    vtables(addr(l.substring(dec ? 4 : 3)), dec);
                } else if (l.startsWith("vtat:")) {
                    String[] p = l.substring(5).split(":");
                    Address s = addr(p[0]);
                    int n = Integer.parseInt(p[1]);
                    int minSize = p.length > 2 ? Integer.parseInt(p[2]) : 0;
                    out.println("\n// vtable at " + s + ", " + n + " slots (decompiling slots of size >= " + minSize + ")");
                    for (int i = 0; i < n; i++, s = s.add(4)) {
                        Address t = toAddr(getInt(s) & 0xffffffffL);
                        Function f = getFunctionAt(t);
                        if (f == null) f = createFunction(t, null);
                        out.println("//   [" + i + "] " + t + (f == null ? "" : " size=" + f.getBody().getNumAddresses()));
                        if (f != null && f.getBody().getNumAddresses() >= minSize) decomp(f, "slot " + i + " of vtable " + p[0]);
                    }
                } else if (l.startsWith("scal:")) {
                    // scal:0xVALUE:0xLO:0xHI[:dec] - instructions in [LO, HI] using VALUE as a scalar
                    // (e.g. a field offset); with :dec, decompile each function found
                    String[] p = l.substring(5).split(":");
                    long want = Long.parseLong(p[0].replace("0x", ""), 16);
                    Address lo = addr(p[1]), hi = addr(p[2]);
                    boolean dec = p.length > 3;
                    out.println("\n// instructions using scalar 0x" + Long.toHexString(want) + " in " + lo + ".." + hi);
                    Map<Function, List<String>> hits = new LinkedHashMap<>();
                    InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(lo, hi), true);
                    while (it.hasNext() && !monitor.isCancelled()) {
                        Instruction ins = it.next();
                        boolean hit = false;
                        for (int i = 0; i < ins.getNumOperands() && !hit; i++)
                            for (Object o : ins.getOpObjects(i))
                                if (o instanceof ghidra.program.model.scalar.Scalar && ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue() == want) hit = true;
                        if (!hit) continue;
                        Function f = getFunctionContaining(ins.getAddress());
                        hits.computeIfAbsent(f, k -> new ArrayList<>()).add(ins.getAddress() + "  " + ins);
                    }
                    for (Map.Entry<Function, List<String>> e : hits.entrySet()) {
                        Function f = e.getKey();
                        out.println("// " + (f == null ? "(no function)" : f.getName() + " @" + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses()));
                        for (String s : e.getValue()) out.println("//     " + s);
                    }
                    if (dec) for (Function f : hits.keySet()) decomp(f, "uses scalar 0x" + Long.toHexString(want));
                } else if (l.startsWith("mnem:")) {
                    // mnem:TEXT:0xLO:0xHI - count instructions whose text contains TEXT, per function
                    String[] p = l.substring(5).split(":");
                    Map<Function, Integer> hits = new LinkedHashMap<>();
                    InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(addr(p[1]), addr(p[2])), true);
                    while (it.hasNext() && !monitor.isCancelled()) {
                        Instruction ins = it.next();
                        if (!ins.toString().contains(p[0])) continue;
                        Function f = getFunctionContaining(ins.getAddress());
                        if (f != null) hits.merge(f, 1, Integer::sum);
                    }
                    out.println("// functions with instructions containing " + p[0]);
                    for (Map.Entry<Function, Integer> e : hits.entrySet())
                        out.println("//   " + e.getKey().getEntryPoint() + " x" + e.getValue() + " size=" + e.getKey().getBody().getNumAddresses());
                } else if (l.startsWith("txt:")) {
                    // txt:ADDR - the ASCII or UTF-16 string at ADDR
                    out.println("// " + l.substring(4) + "  '" + readText(addr(l.substring(4))) + "'");
                } else if (l.startsWith("fstr:")) {
                    // fstr:ADDR - every constant in a function that points at a string (ASCII or
                    // UTF-16), in instruction order: "instruction address  constant  'text'"
                    Function f = func(l.substring(5));
                    out.println("\n// strings used by " + f.getName());
                    InstructionIterator it = currentProgram.getListing().getInstructions(f.getBody(), true);
                    while (it.hasNext() && !monitor.isCancelled()) {
                        Instruction ins = it.next();
                        for (int i = 0; i < ins.getNumOperands(); i++)
                            for (Object o : ins.getOpObjects(i)) {
                                long v;
                                if (o instanceof ghidra.program.model.scalar.Scalar) v = ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue();
                                else if (o instanceof Address) v = ((Address) o).getOffset();
                                else continue;
                                if (v < 0x01300000L || v > 0x01500000L) continue;
                                String s = readText(toAddr(v));
                                if (s != null) out.println("//   " + ins.getAddress() + "  0x" + Long.toHexString(v) + "  '" + s + "'");
                            }
                    }
                } else if (l.startsWith("lst:")) {
                    String[] p = l.substring(4).split(":");
                    out.println("\n// listing " + p[0] + ".." + p[1]);
                    InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(addr(p[0]), addr(p[1])), true);
                    while (it.hasNext()) { Instruction ins = it.next(); out.println("//   " + ins.getAddress() + "  " + ins); }
                } else if (l.startsWith("dw:")) {
                    String[] p = l.substring(3).split(":");
                    Address a = addr(p[0]);
                    int n = Integer.parseInt(p[1]);
                    out.println("\n// dwords at " + a);
                    for (int i = 0; i < n; i++) {
                        Address x = a.add(4L * i);
                        int v = getInt(x);
                        out.println("//   " + x + "  0x" + Integer.toHexString(v) + "  " + v + "  " + Float.intBitsToFloat(v));
                    }
                } else if (l.startsWith("xref:")) {
                    Address a = addr(l.substring(5));
                    out.println("\n// references to " + a);
                    for (Reference r : getReferencesTo(a)) {
                        Function f = getFunctionContaining(r.getFromAddress());
                        out.println("//   from " + r.getFromAddress() + " " + r.getReferenceType() + " in " + (f == null ? "-" : f.getName() + " @" + f.getEntryPoint()));
                    }
                } else if (l.startsWith("utree:")) {
                    // utree:0xFUNC:depth - print the callers tree (who calls FUNC, recursively)
                    String[] p = l.substring(6).split(":");
                    out.println("\n// callers tree of " + p[0]);
                    utree(func(p[0]), Integer.parseInt(p[1]), "", new HashSet<>());
                } else if (l.startsWith("maxl:")) {
                    // maxl:N - set the decompile line limit for the following lines
                    maxLines = Integer.parseInt(l.substring(5));
                } else if (l.startsWith("strs:")) {
                    // strs:needle - list defined strings containing needle with every referencing
                    // function, no decompile
                    String needle = l.substring(5);
                    int n = 0;
                    DataIterator it = currentProgram.getListing().getDefinedData(true);
                    while (it.hasNext() && !monitor.isCancelled() && n < 200) {
                        Data d = it.next();
                        if (!d.hasStringValue()) continue;
                        Object v = d.getValue();
                        if (v == null || !v.toString().contains(needle)) continue;
                        n++;
                        out.println("// str @" + d.getAddress() + " '" + v.toString().replace("\n", " ") + "'");
                        for (Reference r : getReferencesTo(d.getAddress())) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            out.println("//    ref " + r.getFromAddress() + " in " + (f == null ? "-" : f.getName() + " size=" + f.getBody().getNumAddresses()));
                        }
                    }
                } else if (l.startsWith("callees:")) {
                    Function f = func(l.substring(8));
                    out.println("\n// callees of " + f.getName() + " @" + f.getEntryPoint());
                    for (Function c : f.getCalledFunctions(monitor))
                        out.println("//   " + c.getEntryPoint() + " size=" + c.getBody().getNumAddresses() + " callers=" + c.getCallingFunctions(monitor).size());
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

    void utree(Function f, int depth, String ind, Set<Address> seen) {
        if (f == null) return;
        boolean again = seen.contains(f.getEntryPoint());
        out.println("// " + ind + f.getName() + " @" + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses() + (again ? " (again)" : ""));
        if (again || depth <= 0) return;
        seen.add(f.getEntryPoint());
        for (Function c : f.getCallingFunctions(monitor)) utree(c, depth - 1, ind + "  ", seen);
    }

    void ctree(Function f, int depth, String ind, Set<Address> seen) {
        if (f == null) return;
        StringBuilder strs = new StringBuilder();
        // string literals referenced from this function
        AddressSetView body = f.getBody();
        InstructionIterator it = currentProgram.getListing().getInstructions(body, true);
        while (it.hasNext()) {
            Instruction ins = it.next();
            for (Reference r : ins.getReferencesFrom()) {
                Data d = getDataAt(r.getToAddress());
                if (d != null && d.hasStringValue() && d.getValue() != null) {
                    String s = d.getValue().toString();
                    if (s.length() > 3 && strs.length() < 200) strs.append(" \"").append(s).append('"');
                }
            }
        }
        boolean again = seen.contains(f.getEntryPoint());
        out.println("// " + ind + f.getName() + " @" + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses() + (again ? " (again)" : "") + strs);
        if (again || depth <= 0) return;
        seen.add(f.getEntryPoint());
        for (Function c : f.getCalledFunctions(monitor)) {
            if (c.isThunk() || c.isExternal()) continue;
            ctree(c, depth - 1, ind + "  ", seen);
        }
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


    boolean isFuncPtr(Address slot) {
        try {
            int v = getInt(slot);
            Address t = toAddr(v & 0xffffffffL);
            Function f = getFunctionAt(t);
            if (f != null) return true;
            MemoryBlock b = currentProgram.getMemory().getBlock(t);
            return b != null && b.isExecute();
        } catch (Exception e) { return false; }
    }

    void vtables(Address fn, boolean dec) throws Exception {
        out.println("\n// vtables holding " + fn);
        int n = 0;
        for (Reference r : getReferencesTo(fn)) {
            Address slot = r.getFromAddress();
            if (getFunctionContaining(slot) != null) continue; // code ref, not a vtable slot
            if (n++ >= 6) break;
            Address start = slot;
            for (int i = 0; i < 200; i++) {
                Address prev = start.subtract(4);
                if (!isFuncPtr(prev)) break;
                start = prev;
                // a vtable start usually has a reference (the constructor stores it)
                if (getReferencesTo(start).length > 0 && i > 0) { }
            }
            out.println("// vtable candidate starting " + start + " (slot of " + fn + " = " + ((slot.getOffset() - start.getOffset()) / 4) + ")");
            StringBuilder refs = new StringBuilder("//   refs to start: ");
            for (Reference rr : getReferencesTo(start)) {
                Function f = getFunctionContaining(rr.getFromAddress());
                refs.append(rr.getFromAddress()).append(f == null ? "" : "(" + f.getName() + ")").append(' ');
            }
            out.println(refs);
            Address s = start;
            List<Function> fs = new ArrayList<>();
            for (int i = 0; i < 200 && isFuncPtr(s); i++, s = s.add(4)) {
                Address t = toAddr(getInt(s) & 0xffffffffL);
                Function f = getFunctionAt(t);
                if (f == null) f = createFunction(t, null);
                out.println("//   [" + i + "] +0x" + Integer.toHexString(i * 4) + " " + t + (f == null ? "" : " " + f.getName() + " size=" + f.getBody().getNumAddresses()) + (getReferencesTo(s).length > 0 && i > 0 ? "   <- referenced" : ""));
                if (f != null) fs.add(f);
            }
            if (dec) for (Function f : fs) decomp(f, "vtable " + start);
        }
        if (n == 0) out.println("// no data refs to " + fn);
    }

    Address addr(String s) { return toAddr(Long.parseLong(s.replace("0x", ""), 16)); }

    /** A printable ASCII or UTF-16LE string of 2+ characters at `a`, else null. */
    String readText(Address a) {
        try {
            StringBuilder s = new StringBuilder();
            for (int i = 0; i < 200; i++) {
                int b = getByte(a.add(i)) & 0xff;
                if (b == 0) break;
                if (b < 0x20 || b > 0x7e) { s = null; break; }
                s.append((char) b);
            }
            if (s != null && s.length() >= 2) return s.toString();
            StringBuilder w = new StringBuilder();
            for (int i = 0; i < 200; i++) {
                int lo = getByte(a.add(2 * i)) & 0xff, hi = getByte(a.add(2 * i + 1)) & 0xff;
                if (lo == 0 && hi == 0) break;
                if (hi != 0 || lo < 0x20 || lo > 0x7e) return null;
                w.append((char) lo);
            }
            return w.length() >= 2 ? "L:" + w : null;
        } catch (Exception e) {
            return null;
        }
    }

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
