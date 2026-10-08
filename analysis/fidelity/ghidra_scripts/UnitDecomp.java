// Fidelity 0-D worker (copy of FidDecomp + SptDecomp bytes:, plus wbytes: for UTF-16LE): DecompTargets (worker1) plus vtable, data and xref commands.
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

public class UnitDecomp extends GhidraScript {
    DecompInterface di;
    PrintWriter out;
    Set<Address> done = new HashSet<>();
    int maxLines = 260;
    Map<Long, List<Address>> scalars, ptrs;

    // The printable ASCII string at a pointer (>= 2 chars, NUL-terminated), else null.
    String ptrstr(long v) {
        if (v < 0x400000L || v >= 0x2000000L) return null;
        try {
            Address a = toAddr(v);
            if (!currentProgram.getMemory().contains(a)) return null;
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < 80; i++) {
                int c = getByte(a.add(i)) & 0xff;
                if (c == 0) return sb.length() >= 2 ? sb.toString() : null;
                if (c < 0x20 || c > 0x7e) return null;
                sb.append((char) c);
            }
            return sb.toString() + "...";
        } catch (Exception e) { return null; }
    }

    // Every scalar operand in [0x00400000, 0x02000000) -> instruction addresses (built once).
    Map<Long, List<Address>> scalarIndex() {
        if (scalars != null) return scalars;
        scalars = new HashMap<>();
        InstructionIterator it = currentProgram.getListing().getInstructions(true);
        while (it.hasNext()) {
            Instruction ins = it.next();
            for (int i = 0; i < ins.getNumOperands(); i++)
                for (Object o : ins.getOpObjects(i))
                    if (o instanceof ghidra.program.model.scalar.Scalar) {
                        long v = ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue();
                        if (v >= 0x400000L && v < 0x2000000L) scalars.computeIfAbsent(v, k -> new ArrayList<>()).add(ins.getAddress());
                    }
        }
        return scalars;
    }

    // Every aligned dword in initialized non-executable memory that looks like a pointer -> its address.
    Map<Long, List<Address>> ptrIndex() {
        if (ptrs != null) return ptrs;
        ptrs = new HashMap<>();
        for (MemoryBlock b : currentProgram.getMemory().getBlocks()) {
            if (!b.isInitialized() || b.isExecute()) continue;
            byte[] buf = new byte[(int) b.getSize()];
            try { b.getBytes(b.getStart(), buf); } catch (Exception e) { continue; }
            long base = b.getStart().getOffset();
            for (int i = (int) ((4 - (base & 3)) & 3); i + 4 <= buf.length; i += 4) {
                long v = (buf[i] & 0xffL) | ((buf[i + 1] & 0xffL) << 8) | ((buf[i + 2] & 0xffL) << 16) | ((buf[i + 3] & 0xffL) << 24);
                if (v >= 0x400000L && v < 0x2000000L) ptrs.computeIfAbsent(v, k -> new ArrayList<>()).add(b.getStart().add(i));
            }
        }
        return ptrs;
    }

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
                } else if (l.startsWith("bytes:") || l.startsWith("wbytes:") || l.startsWith("nbytes:") || l.startsWith("nwbytes:")) {
                    // bytes:text / wbytes:text (UTF-16LE) - find the text anywhere in memory (NUL-terminated
                    // match when the text ends with "$"), list references (data refs or scalar operands)
                    // and decompile up to 3 users per hit. nbytes:/nwbytes: = list only, no decompile.
                    boolean wide = l.startsWith("wbytes:") || l.startsWith("nwbytes:");
                    boolean nodec = l.startsWith("n");
                    String needle = l.substring(l.indexOf(':') + 1);
                    boolean term = needle.endsWith("$");
                    if (term) needle = needle.substring(0, needle.length() - 1);
                    byte[] nb0 = needle.getBytes(wide ? "UTF-16LE" : "US-ASCII");
                    byte[] nb = term ? Arrays.copyOf(nb0, nb0.length + (wide ? 2 : 1)) : nb0;
                    Address a = currentProgram.getMinAddress();
                    int hits = 0;
                    while (hits < 6) {
                        a = currentProgram.getMemory().findBytes(a, nb, null, true, monitor);
                        if (a == null) break;
                        hits++;
                        out.println("// " + (wide ? "w" : "") + "bytes '" + needle + "' at " + a);
                        int n = 0;
                        List<Function> users = new ArrayList<>();
                        for (Reference r : getReferencesTo(a)) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            out.println("//   ref from " + r.getFromAddress() + " in " + (f == null ? "-" : f.getName()));
                            if (f != null && !users.contains(f)) users.add(f);
                            n++;
                        }
                        if (n == 0) {
                            long want = a.getOffset();
                            for (Address ia : scalarIndex().getOrDefault(want, Collections.emptyList())) {
                                Function f = getFunctionContaining(ia);
                                out.println("//   scalar use at " + ia + " in " + (f == null ? "-" : f.getName()));
                                if (f != null && !users.contains(f)) users.add(f);
                                n++;
                            }
                            for (Address pa : ptrIndex().getOrDefault(want, Collections.emptyList())) {
                                StringBuilder sb = new StringBuilder("//   pointer at " + pa + " refs:");
                                for (Reference r : getReferencesTo(pa)) {
                                    Function f = getFunctionContaining(r.getFromAddress());
                                    sb.append(" ").append(r.getFromAddress()).append(f == null ? "" : "(" + f.getName() + ")");
                                }
                                out.println(sb);
                                n++;
                            }
                        }
                        if (!nodec) for (int k = 0; k < users.size() && k < 3; k++) decomp(users.get(k), "user of '" + needle + "'");
                        a = a.add(1);
                    }
                    if (hits == 0) out.println("// not found: " + needle);
                } else if (l.startsWith("hexfind:")) {
                    // hexfind:0a0000003e000000 - every occurrence of the byte pattern (max 20), with
                    // the references to each hit
                    String h = l.substring(8);
                    byte[] nb = new byte[h.length() / 2];
                    for (int i = 0; i < nb.length; i++) nb[i] = (byte) Integer.parseInt(h.substring(2 * i, 2 * i + 2), 16);
                    out.println("\n// hexfind " + h);
                    Address a = currentProgram.getMinAddress();
                    for (int hits = 0; hits < 20; hits++) {
                        a = currentProgram.getMemory().findBytes(a, nb, null, true, monitor);
                        if (a == null) break;
                        StringBuilder sb = new StringBuilder("//   " + a);
                        for (Reference r : getReferencesTo(a)) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            sb.append(" ").append(r.getFromAddress()).append(f == null ? "" : "(" + f.getName() + ")");
                        }
                        out.println(sb);
                        a = a.add(1);
                    }
                } else if (l.startsWith("strlist:")) {
                    // strlist:needle - every defined string containing needle (case-insensitive),
                    // with the functions referencing it (no decompile)
                    String needle = l.substring(8).toLowerCase();
                    out.println("\n// strings containing '" + needle + "'");
                    DataIterator it = currentProgram.getListing().getDefinedData(true);
                    int n = 0;
                    while (it.hasNext() && n < 300) {
                        Data d = it.next();
                        if (!d.hasStringValue()) continue;
                        Object v = d.getValue();
                        if (v == null || !v.toString().toLowerCase().contains(needle)) continue;
                        StringBuilder sb = new StringBuilder("//   " + d.getAddress() + " \"" + v.toString().replace("\n", "\\n") + "\"");
                        for (Reference r : getReferencesTo(d.getAddress())) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            sb.append(" ").append(f == null ? r.getFromAddress().toString() : f.getName());
                        }
                        out.println(sb);
                        n++;
                    }
                } else if (l.startsWith("cooc:")) {
                    // cooc:0xLO:0xHI:v1,v2,...[:dec] - functions in LO..HI whose instructions use every
                    // listed scalar (hex) as an immediate operand; :dec decompiles them (max 6)
                    String[] p = l.substring(5).split(":");
                    String[] vs = p[2].split(",");
                    long[] want = new long[vs.length];
                    for (int i = 0; i < vs.length; i++) want[i] = Long.parseLong(vs[i].replace("0x", ""), 16);
                    Map<Function, Integer> masks = new LinkedHashMap<>();
                    InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(addr(p[0]), addr(p[1])), true);
                    while (it.hasNext()) {
                        Instruction ins = it.next();
                        for (int i = 0; i < ins.getNumOperands(); i++)
                            for (Object o : ins.getOpObjects(i))
                                if (o instanceof ghidra.program.model.scalar.Scalar) {
                                    long v = ((ghidra.program.model.scalar.Scalar) o).getUnsignedValue();
                                    for (int k = 0; k < want.length; k++)
                                        if (v == want[k]) {
                                            Function f = getFunctionContaining(ins.getAddress());
                                            if (f != null) masks.merge(f, 1 << k, (a, b) -> a | b);
                                        }
                                }
                    }
                    int full = (1 << want.length) - 1, n = 0;
                    out.println("\n// functions using all of " + p[2] + " in " + p[0] + ".." + p[1]);
                    List<Function> hitsF = new ArrayList<>();
                    for (Map.Entry<Function, Integer> e : masks.entrySet())
                        if (e.getValue() == full) {
                            out.println("//   " + e.getKey().getName() + " size=" + e.getKey().getBody().getNumAddresses());
                            hitsF.add(e.getKey());
                        }
                    if (p.length > 3) for (Function f : hitsF) if (n++ < 6) decomp(f, "cooc " + p[2]);
                } else if (l.startsWith("funcs:")) {
                    // funcs:0xLO:0xHI - functions in the range with size and caller count
                    String[] p = l.substring(6).split(":");
                    for (Function f : currentProgram.getFunctionManager().getFunctions(new AddressSet(addr(p[0]), addr(p[1])), true)) {
                        int nc = f.getCallingFunctions(monitor).size();
                        out.println("// " + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses() + " callers=" + nc + " " + f.getName());
                    }
                } else if (l.startsWith("ptab:")) {
                    // ptab:0xADDR:N:STRIDE - N records of STRIDE bytes: every dword, shown as the
                    // ASCII string it points to when it is a pointer to text, else hex/int
                    String[] p = l.substring(5).split(":");
                    Address a = addr(p[0]);
                    int n = Integer.parseInt(p[1]), stride = Integer.parseInt(p[2]);
                    out.println("\n// table at " + a + ", " + n + " x " + stride);
                    for (int i = 0; i < n; i++) {
                        StringBuilder sb = new StringBuilder("//   " + a.add((long) i * stride) + " [" + i + "]");
                        for (int k = 0; k + 4 <= stride; k += 4) {
                            int v = getInt(a.add((long) i * stride + k));
                            String s = ptrstr(v & 0xffffffffL);
                            sb.append(s != null ? " \"" + s + "\"" : " " + Integer.toHexString(v));
                        }
                        StringBuilder rs = new StringBuilder();
                        for (Reference r : getReferencesTo(a.add((long) i * stride))) {
                            Function f = getFunctionContaining(r.getFromAddress());
                            rs.append(" ").append(r.getFromAddress()).append(f == null ? "" : "(" + f.getName() + ")");
                        }
                        if (rs.length() > 0) sb.append("   refs:").append(rs);
                        out.println(sb);
                    }
                } else if (l.startsWith("find:")) {
                    // find:0xDWORD - every 4-byte-aligned occurrence of DWORD in initialized
                    // non-executable memory, with the code references to each occurrence
                    long want = Long.parseLong(l.substring(5).replace("0x", ""), 16);
                    out.println("\n// occurrences of dword 0x" + Long.toHexString(want));
                    int n = 0;
                    for (MemoryBlock b : currentProgram.getMemory().getBlocks()) {
                        if (!b.isInitialized() || b.isExecute()) continue;
                        for (Address a = b.getStart(); a.compareTo(b.getEnd().subtract(3)) < 0 && n < 40; a = a.add(4)) {
                            if ((getInt(a) & 0xffffffffL) != want) continue;
                            n++;
                            StringBuilder sb = new StringBuilder("//   " + a + ":");
                            int k = 0;
                            for (Reference r : getReferencesTo(a)) {
                                Function f = getFunctionContaining(r.getFromAddress());
                                if (k++ < 12) sb.append(" ").append(r.getFromAddress()).append(f == null ? "" : "(" + f.getName() + ")");
                            }
                            out.println(sb);
                        }
                    }
                } else if (l.startsWith("ins:")) {
                    // ins:0xLO:0xHI:regex - instructions in LO..HI whose text matches regex
                    String[] p = l.substring(4).split(":", 3);
                    java.util.regex.Pattern pat = java.util.regex.Pattern.compile(p[2]);
                    out.println("\n// instructions matching /" + p[2] + "/ in " + p[0] + ".." + p[1]);
                    InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(addr(p[0]), addr(p[1])), true);
                    int n = 0;
                    while (it.hasNext() && n < 400) {
                        Instruction ins = it.next();
                        if (!pat.matcher(ins.toString()).find()) continue;
                        Function f = getFunctionContaining(ins.getAddress());
                        out.println("//   " + ins.getAddress() + "  " + ins + "   in " + (f == null ? "-" : f.getName() + " size=" + f.getBody().getNumAddresses()));
                        n++;
                    }
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
