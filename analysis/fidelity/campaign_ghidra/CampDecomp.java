// Campaign fidelity worker (copy of analysis/ai AiDecomp): DecompTargets (worker1) plus vtable, data and xref commands.
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

public class CampDecomp extends GhidraScript {
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
                } else if (l.startsWith("str:") || l.startsWith("strx:")) {
                    boolean exact = l.startsWith("strx:");
                    String needle = l.substring(exact ? 5 : 4);
                    int n = 0;
                    DataIterator it = currentProgram.getListing().getDefinedData(true);
                    while (it.hasNext() && !monitor.isCancelled()) {
                        Data d = it.next();
                        if (!d.hasStringValue()) continue;
                        Object v = d.getValue();
                        if (v == null || (exact ? !v.toString().equals(needle) : !v.toString().contains(needle))) continue;
                        for (Reference r : getReferencesTo(d.getAddress())) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            if (f != null && n++ < 10) decomp(f, "string '" + v.toString().replace("\n", " ") + "' @" + d.getAddress());
                        }
                        if (n >= 10) break;
                    }
                    if (n == 0) out.println("// no functions found for str:" + needle);
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
                } else if (l.startsWith("allpat:")) {
                    // allpat:0xLO:0xHI:re1;;re2;;... - functions in LO..HI whose instruction text
                    // matches every regex (e.g. "CALL dword ptr \[E.X \+ 0x30\]")
                    String[] p = l.substring(7).split(":", 3);
                    Address lo = addr(p[0]), hi = addr(p[1]);
                    String[] res = p[2].split(";;");
                    java.util.regex.Pattern[] pats = new java.util.regex.Pattern[res.length];
                    for (int i = 0; i < res.length; i++) pats[i] = java.util.regex.Pattern.compile(res[i]);
                    out.println("\n// functions in " + lo + ".." + hi + " matching all of " + p[2]);
                    // key = function entry; outside defined functions, the nearest earlier
                    // instruction that something references (INFERRED entry), else "?"
                    Map<Address, boolean[]> seen = new TreeMap<>();
                    InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(lo, hi), true);
                    Address curEntry = null;
                    while (it.hasNext() && !monitor.isCancelled()) {
                        Instruction ins = it.next();
                        Function f = getFunctionContaining(ins.getAddress());
                        if (f != null) curEntry = f.getEntryPoint();
                        else if (currentProgram.getReferenceManager().hasReferencesTo(ins.getAddress())) {
                            Instruction prev = ins.getPrevious();
                            String pm = prev == null ? "" : prev.getMnemonicString();
                            if (curEntry == null || pm.equals("RET") || pm.equals("INT3") || pm.equals("JMP") || pm.equals("NOP")) curEntry = ins.getAddress();
                        }
                        String txt = ins.toString();
                        for (int i = 0; i < pats.length; i++) if (pats[i].matcher(txt).find() && curEntry != null)
                            seen.computeIfAbsent(curEntry, k -> new boolean[pats.length])[i] = true;
                    }
                    out.println("// entries with any match: " + seen.size());
                    for (Map.Entry<Address, boolean[]> e : seen.entrySet()) {
                        boolean all = true;
                        for (boolean b : e.getValue()) all &= b;
                        if (all) out.println("// entry " + e.getKey() + (getFunctionAt(e.getKey()) == null ? " (no function)" : ""));
                    }
                } else if (l.startsWith("vars:")) {
                    String[] p = l.substring(5).split(":");
                    varsScan(addr(p[0]), addr(p[1]));
                } else if (l.startsWith("vdisp:")) {
                    String[] p = l.substring(6).split(":");
                    vdispScan(addr(p[0]), addr(p[1]), Long.parseLong(p[2].replace("0x", ""), 16));
                } else if (l.startsWith("callsites:")) {
                    String[] p = l.substring(10).split(":");
                    callSites(addr(p[0]), Integer.parseInt(p[1]));
                } else if (l.startsWith("ptrs:")) {
                    // ptrs:0xADDR:N - N dwords with the ASCII string each points at
                    String[] p = l.substring(5).split(":");
                    Address a = addr(p[0]);
                    out.println("\n// pointers at " + a);
                    for (int i = 0; i < Integer.parseInt(p[1]); i++) {
                        Address x = a.add(4L * i);
                        long v = getInt(x) & 0xffffffffL;
                        out.println("//   " + x + " 0x" + Long.toHexString(v) + " " + cstr(v));
                    }
                } else if (l.startsWith("pushcall:")) {
                    pushCall(l.substring(9));
                } else if (l.startsWith("dispset:")) {
                    String[] p = l.substring(8).split(":");
                    dispSet(addr(p[0]), addr(p[1]), Integer.parseInt(p[2]), Integer.parseInt(p[3]));
                } else if (l.startsWith("bytes:")) {
                    // bytes:HEX - raw byte pattern; prints each hit and the 40 bytes before it as a listing
                    String hex = l.substring(6);
                    byte[] pat = new byte[hex.length() / 2];
                    for (int i = 0; i < pat.length; i++) pat[i] = (byte) Integer.parseInt(hex.substring(2 * i, 2 * i + 2), 16);
                    out.println("\n// bytes " + hex);
                    Address s = currentProgram.getMinAddress();
                    for (int k = 0; k < 12; k++) {
                        Address a = currentProgram.getMemory().findBytes(s, pat, null, true, monitor);
                        if (a == null) break;
                        s = a.add(1);
                        Function f = getFunctionContaining(a);
                        out.println("//   hit " + a + (f == null ? "" : " in " + f.getName()));
                        InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(a.subtract(48), a.add(24)), true);
                        while (it.hasNext()) { Instruction ins = it.next(); out.println("//     " + ins.getAddress() + "  " + ins + strOperands(ins)); }
                    }
                } else if (l.startsWith("mem:")) {
                    memFind(l.substring(4));
                } else if (l.startsWith("lst:")) {
                    String[] p = l.substring(4).split(":");
                    out.println("\n// listing " + p[0] + ".." + p[1]);
                    InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(addr(p[0]), addr(p[1])), true);
                    while (it.hasNext()) { Instruction ins = it.next(); out.println("//   " + ins.getAddress() + "  " + ins + strOperands(ins)); }
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
                } else if (l.startsWith("fsmscan:")) {
                    // fsmscan:0xLO:0xHI - find FSM state descriptors {update, entry, exit, x, name getter}
                    // in a data range; print name, entry/exit targets, the update's calls and who
                    // refers to the descriptor (transition tables -> thunks -> vtable slots)
                    String[] p = l.substring(8).split(":");
                    fsmScan(addr(p[0]), addr(p[1]));
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

    // vars:0xLO:0xHI - campaign variable registrations (PUSH "name"; MOV ECX,obj; CALL reg) in
    // LO..HI; then one pass over all code listing the functions that use obj..obj+7
    void varsScan(Address lo, Address hi) throws Exception {
        Map<Long, String> objs = new TreeMap<>();
        String last = null;
        InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(lo, hi), true);
        while (it.hasNext()) {
            Instruction ins = it.next();
            String t = ins.toString();
            if (t.startsWith("PUSH 0x")) {
                long v = Long.parseLong(t.substring(7), 16);
                last = cstr(v);
            } else if (t.startsWith("MOV ECX,0x") && last != null) {
                objs.put(Long.parseLong(t.substring(10), 16), last);
                last = null;
            }
        }
        out.println("\n// vars in " + lo + ".." + hi + ": " + objs.size());
        Map<Long, String> byAddr = new HashMap<>();
        for (Map.Entry<Long, String> e : objs.entrySet())
            for (int k = 0; k < 8; k++) byAddr.put(e.getKey() + k, e.getValue() + (k == 0 ? "" : "+" + k));
        Map<String, Set<String>> uses = new TreeMap<>();
        InstructionIterator all = currentProgram.getListing().getInstructions(true);
        while (all.hasNext() && !monitor.isCancelled()) {
            Instruction ins = all.next();
            if (ins.getAddress().compareTo(lo) >= 0 && ins.getAddress().compareTo(hi) <= 0) continue;
            for (int i = 0; i < ins.getNumOperands(); i++)
                for (Object o : ins.getOpObjects(i)) {
                    long v = -1;
                    if (o instanceof ghidra.program.model.scalar.Scalar) v = ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue();
                    else if (o instanceof Address) v = ((Address) o).getOffset();
                    String n = byAddr.get(v);
                    if (n == null) continue;
                    Function f = getFunctionContaining(ins.getAddress());
                    uses.computeIfAbsent(n, k -> new TreeSet<>()).add(f == null ? "nofunc@" + ins.getAddress() : f.getEntryPoint() + "(" + f.getBody().getNumAddresses() + ")");
                }
        }
        for (Map.Entry<Long, String> e : objs.entrySet()) {
            out.println("// " + Long.toHexString(e.getKey()) + " " + e.getValue());
            for (Map.Entry<String, Set<String>> u : uses.entrySet())
                if (u.getKey().equals(e.getValue()) || u.getKey().startsWith(e.getValue() + "+"))
                    out.println("//     " + u.getKey() + ": " + u.getValue());
        }
    }

    // vdisp:0xLO:0xHI:0xBASE - names from the registrations in LO..HI (from the first var on, in
    // order), then every instruction whose operand scalar is BASE + 4*index (a field of the
    // campaign model's variable array): mnemonic, function
    void vdispScan(Address lo, Address hi, long base) throws Exception {
        List<String> names = new ArrayList<>();
        String last = null;
        InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(lo, hi), true);
        while (it.hasNext()) {
            Instruction ins = it.next();
            String t = ins.toString();
            if (t.startsWith("PUSH 0x")) last = cstr(Long.parseLong(t.substring(7), 16));
            else if (t.startsWith("MOV ECX,0x") && last != null) { names.add(last); last = null; }
            else if (t.startsWith("CALL")) last = null;
        }
        Map<Long, Integer> idx = new HashMap<>();
        for (int i = 0; i < names.size(); i++) idx.put(base + 4L * i, i);
        Map<Integer, List<String>> hits = new TreeMap<>();
        InstructionIterator all = currentProgram.getListing().getInstructions(true);
        while (all.hasNext() && !monitor.isCancelled()) {
            Instruction ins = all.next();
            for (int i = 0; i < ins.getNumOperands(); i++)
                for (Object o : ins.getOpObjects(i)) {
                    if (!(o instanceof ghidra.program.model.scalar.Scalar)) continue;
                    Integer k = idx.get(((ghidra.program.model.scalar.Scalar) o).getUnsignedValue());
                    if (k == null) continue;
                    Function f = getFunctionContaining(ins.getAddress());
                    hits.computeIfAbsent(k, x -> new ArrayList<>()).add(ins.getAddress() + " " + ins + "  in " + (f == null ? "-" : f.getEntryPoint().toString()));
                }
        }
        out.println("\n// vdisp base 0x" + Long.toHexString(base) + ", " + names.size() + " names");
        for (int i = 0; i < names.size(); i++) {
            out.println("// [" + i + "] +0x" + Long.toHexString(base + 4L * i) + " " + names.get(i));
            List<String> h = hits.get(i);
            if (h == null) continue;
            int n = 0;
            for (String s : h) if (n++ < 40) out.println("//     " + s);
            if (h.size() > 40) out.println("//     ... " + h.size() + " total");
        }
    }

    // callsites:0xFUNC:N - every call to FUNC with the N instructions before it (argument pushes)
    void callSites(Address fn, int n) throws Exception {
        out.println("\n// call sites of " + fn);
        for (Reference r : getReferencesTo(fn)) {
            if (!r.getReferenceType().isCall()) continue;
            Address at = r.getFromAddress();
            Function f = getFunctionContaining(at);
            StringBuilder sb = new StringBuilder("//   " + at + " in " + (f == null ? "-" : f.getEntryPoint().toString()) + ":");
            List<String> prev = new ArrayList<>();
            Instruction ins = getInstructionAt(at);
            for (int i = 0; i < n && ins != null; i++) {
                ins = ins.getPrevious();
                if (ins != null) prev.add(0, ins.toString());
            }
            for (String s : prev) sb.append(" | ").append(s);
            out.println(sb);
        }
    }

    // pushcall:0xV1,0xV2,... - histogram of CALL targets that follow a PUSH of one of the values
    // within 4 instructions (finds getters taking an enum index)
    void pushCall(String list) throws Exception {
        Set<Long> vals = new HashSet<>();
        for (String s : list.split(",")) vals.add(Long.parseLong(s.replace("0x", ""), 16));
        Map<String, Integer> hist = new TreeMap<>();
        Map<String, List<String>> where = new HashMap<>();
        InstructionIterator all = currentProgram.getListing().getInstructions(true);
        while (all.hasNext() && !monitor.isCancelled()) {
            Instruction ins = all.next();
            if (!ins.getMnemonicString().equals("PUSH") || ins.getNumOperands() != 1) continue;
            Object[] o = ins.getOpObjects(0);
            if (o.length != 1 || !(o[0] instanceof ghidra.program.model.scalar.Scalar)) continue;
            long v = ((ghidra.program.model.scalar.Scalar) o[0]).getUnsignedValue();
            if (!vals.contains(v)) continue;
            Instruction n = ins.getNext();
            for (int i = 0; i < 4 && n != null; i++, n = n.getNext()) {
                if (n.getMnemonicString().equals("CALL")) {
                    String t = n.getDefaultOperandRepresentation(0) + " push " + Long.toHexString(v);
                    hist.merge(t, 1, Integer::sum);
                    Function f = getFunctionContaining(n.getAddress());
                    where.computeIfAbsent(t, k -> new ArrayList<>()).add(f == null ? n.getAddress().toString() : f.getEntryPoint().toString());
                    break;
                }
                if (n.getMnemonicString().equals("PUSH")) break;
            }
        }
        out.println("\n// pushcall " + list);
        for (Map.Entry<String, Integer> e : hist.entrySet()) out.println("//   " + e.getKey() + " x" + e.getValue() + " in " + where.get(e.getKey()));
    }

    // dispset:0xLO:0xHI:FIRST:COUNT - instructions in LO..HI with a memory displacement equal to
    // 4*i for i in FIRST..FIRST+COUNT-1 (a field of a float array indexed by a constant), grouped
    // by function with the indices used
    void dispSet(Address lo, Address hi, int first, int count) throws Exception {
        Map<Function, TreeSet<Integer>> byFn = new LinkedHashMap<>();
        Map<Function, List<String>> lines = new LinkedHashMap<>();
        InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(lo, hi), true);
        while (it.hasNext() && !monitor.isCancelled()) {
            Instruction ins = it.next();
            String t = ins.toString();
            if (!t.contains("ptr [")) continue;
            for (int i = 0; i < ins.getNumOperands(); i++)
                for (Object o : ins.getOpObjects(i)) {
                    if (!(o instanceof ghidra.program.model.scalar.Scalar)) continue;
                    long v = ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue();
                    if (v % 4 != 0) continue;
                    long idx = v / 4;
                    if (idx < first || idx >= first + count) continue;
                    if (!t.matches(".*\\[E[A-Z]{2} \\+ 0x[0-9a-f]+\\].*")) continue;
                    Function f = getFunctionContaining(ins.getAddress());
                    if (f == null) continue;
                    byFn.computeIfAbsent(f, k -> new TreeSet<>()).add((int) idx);
                    lines.computeIfAbsent(f, k -> new ArrayList<>()).add(ins.getAddress() + " " + t);
                }
        }
        out.println("\n// dispset " + lo + ".." + hi + " indices " + first + ".." + (first + count - 1));
        for (Map.Entry<Function, TreeSet<Integer>> e : byFn.entrySet()) {
            if (e.getValue().size() < 3) continue;
            out.println("// " + e.getKey().getEntryPoint() + " size=" + e.getKey().getBody().getNumAddresses() + " idx " + e.getValue());
            for (String s : lines.get(e.getKey())) out.println("//     " + s);
        }
    }

    // mem:needle - find the NUL-terminated ASCII string in memory (defined or not), list code refs
    // to it and to any dword that points at it (pointer tables), decompiling up to 6 functions
    void memFind(String needle) throws Exception {
        byte[] pat = (needle + "\0").getBytes("US-ASCII");
        out.println("\n// mem: " + needle);
        Address start = currentProgram.getMinAddress();
        int hits = 0, dec = 0;
        while (hits < 8) {
            Address a = currentProgram.getMemory().findBytes(start, pat, null, true, monitor);
            if (a == null) break;
            start = a.add(1);
            if (getByte(a.subtract(1)) != 0) continue; // must start a string
            hits++;
            out.println("//   string at " + a);
            List<Address> targets = new ArrayList<>();
            targets.add(a);
            long o = a.getOffset();
            byte[] le = new byte[] {(byte) o, (byte) (o >> 8), (byte) (o >> 16), (byte) (o >> 24)};
            Address ps = currentProgram.getMinAddress();
            for (int k = 0; k < 8; k++) {
                Address p = currentProgram.getMemory().findBytes(ps, le, null, true, monitor);
                if (p == null) break;
                ps = p.add(1);
                Function pf = getFunctionContaining(p);
                out.println("//   dword pointer at " + p + (pf == null ? " (data)" : " in " + pf.getName() + " @" + pf.getEntryPoint()));
                if (pf == null) targets.add(p);
                else if (dec++ < 6) decomp(pf, "immediate ptr to '" + needle + "'");
            }
            for (Address t : targets) for (Reference r : getReferencesTo(t)) {
                Function f = getFunctionContaining(r.getFromAddress());
                out.println("//   ref to " + t + " from " + r.getFromAddress() + (f == null ? "" : " in " + f.getName() + " @" + f.getEntryPoint()));
                if (f != null && dec++ < 6) decomp(f, "ref to '" + needle + "' via " + t);
            }
        }
        if (hits == 0) out.println("//   not found");
    }

    boolean isCode(long v) {
        MemoryBlock b = currentProgram.getMemory().getBlock(toAddr(v));
        return b != null && b.isExecute();
    }

    long dword(Address a) throws Exception { return getInt(a) & 0xffffffffL; }

    String cstr(long v) {
        try {
            StringBuilder sb = new StringBuilder();
            Address a = toAddr(v);
            for (int i = 0; i < 80; i++) {
                int c = getByte(a.add(i)) & 0xff;
                if (c == 0) break;
                if (c < 32 || c > 126) return null;
                sb.append((char) c);
            }
            return sb.toString();
        } catch (Exception e) { return null; }
    }

    // UTF-16LE string at v, or null
    String wstr(long v) {
        try {
            StringBuilder sb = new StringBuilder();
            Address a = toAddr(v);
            for (int i = 0; i < 120; i++) {
                int c = (getByte(a.add(2 * i)) & 0xff) | ((getByte(a.add(2 * i + 1)) & 0xff) << 8);
                if (c == 0) break;
                if (c < 32 || c > 126) return null;
                sb.append((char) c);
            }
            return sb.length() < 2 ? null : sb.toString();
        } catch (Exception e) { return null; }
    }

    // operand constants that point at an ASCII or UTF-16 string, as a comment
    String strOperands(Instruction ins) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < ins.getNumOperands(); i++)
            for (Object o : ins.getOpObjects(i)) {
                long v = -1;
                if (o instanceof ghidra.program.model.scalar.Scalar) v = ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue();
                else if (o instanceof Address) v = ((Address) o).getOffset();
                if (v < 0x00400000L || v > 0x02000000L) continue;
                String s = cstr(v);
                if (s == null || s.length() < 3) s = wstr(v);
                if (s != null && s.length() >= 3) sb.append("   ; \"").append(s).append('"');
            }
        return sb.toString();
    }

    Instruction insAt(Address a) {
        Instruction ins = getInstructionAt(a);
        if (ins == null) { disassemble(a); ins = getInstructionAt(a); }
        return ins;
    }

    // condensed instruction trace of a small thunk: calls, branches and returns, up to the next getter
    String thunkCalls(long start, int maxIns) {
        StringBuilder sb = new StringBuilder();
        Instruction ins = insAt(toAddr(start));
        for (int i = 0; i < maxIns && ins != null; i++) {
            String s = ins.toString();
            if (i > 0 && s.startsWith("MOV EAX,[0x")) break; // next state's getter
            if (s.startsWith("CALL") || s.startsWith("LEA EAX,[E") || s.startsWith("J") || s.startsWith("RET") || s.startsWith("XOR EAX")) {
                sb.append(" | ").append(ins.getAddress()).append(' ').append(s);
            }
            ins = insAt(ins.getMaxAddress().add(1));
        }
        return sb.toString();
    }

    void fsmScan(Address lo, Address hi) throws Exception {
        out.println("\n// FSM state descriptors in " + lo + ".." + hi);
        for (Address a = lo; a.compareTo(hi) < 0; a = a.add(4)) {
            long u = dword(a), en = dword(a.add(4)), ex = dword(a.add(8)), g = dword(a.add(16));
            if (!isCode(u) || !isCode(en) || !isCode(ex) || !isCode(g)) continue;
            if (Math.abs(u - en) > 0x400 || Math.abs(g - en) > 0x400) continue;
            Instruction gi = insAt(toAddr(g));
            if (gi == null || !gi.toString().startsWith("MOV EAX,[0x")) continue;
            String name = "?";
            Address ga = null;
            for (Object o : gi.getOpObjects(1)) {
                if (o instanceof Address) ga = (Address) o;
                else if (o instanceof ghidra.program.model.scalar.Scalar) ga = toAddr(((ghidra.program.model.scalar.Scalar) o).getUnsignedValue());
            }
            if (ga != null) name = cstr(dword(ga)) + " (via " + ga + ")";
            out.println("\n// DESC " + a + "  name=" + name);
            out.println("//   entry " + Long.toHexString(en) + thunkCalls(en, 12));
            out.println("//   exit  " + Long.toHexString(ex) + thunkCalls(ex, 12));
            out.println("//   x     " + Long.toHexString(dword(a.add(12))) + thunkCalls(dword(a.add(12)), 12));
            out.println("//   update " + Long.toHexString(u) + thunkCalls(u, 80));
            for (Reference r : getReferencesTo(a)) {
                Address holder = r.getFromAddress();
                StringBuilder sb = new StringBuilder("//   ref from " + holder);
                for (Reference r2 : getReferencesTo(holder)) {
                    Address th = r2.getFromAddress();
                    Function tf = getFunctionContaining(th);
                    Address te = tf == null ? th : tf.getEntryPoint();
                    sb.append(" <- thunk ").append(te);
                    for (Reference r3 : getReferencesTo(te)) {
                        if (getFunctionContaining(r3.getFromAddress()) == null) sb.append(" [slot@").append(r3.getFromAddress()).append(']');
                    }
                }
                out.println(sb);
            }
            a = a.add(16);
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
