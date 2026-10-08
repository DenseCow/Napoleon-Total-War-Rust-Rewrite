// For each DB row-reader function, list the stream-read calls in address order and the
// BUILDER struct offset each read writes to. Output: TSV lines.
// Usage: postScript args <readers_file (table<TAB>readerVA per line)> <out_tsv>
// The callee prototypes are fixed in memory first (the project is opened -readOnly, so changes are discarded).
//@category NapoleonRE
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.pcode.*;
import ghidra.program.model.lang.Register;
import ghidra.program.model.data.*;
import ghidra.program.model.symbol.SourceType;
import java.io.*;
import java.nio.file.*;
import java.util.*;

public class DbReaderScan extends GhidraScript {
    static final long READ_STRING = 0x00dd7e50L;   // cdecl (stream, dst) : u16 len + UTF-16
    static final long READ_RAW = 0x00687cf0L;      // thiscall stream->(dst, n) : 4-byte copy
    static final long READ_BYTE = 0x00dbb6b0L;     // thiscall stream->(dst, n) : 1-byte copy
    static final long READ_U16 = 0x00687ca0L;      // thiscall stream->(dst, n)
    static final long OPERATOR_ARROW = 0x00445230L;
    DecompInterface di;
    PrintWriter out;

    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        out = new PrintWriter(new FileWriter(args[1], false));
        out.println("# Ghidra-derived DB row-reader field list (reconstructed). table\treader\tidx\tcall_addr\tprimitive\tdst\tsize\tnote");
        fixProto(READ_STRING, "__cdecl", 2);
        fixProto(READ_RAW, "__thiscall", 2);
        fixProto(READ_BYTE, "__thiscall", 2);
        fixProto(READ_U16, "__thiscall", 2);
        di = new DecompInterface();
        di.setOptions(new DecompileOptions());
        di.toggleCCode(false);
        di.openProgram(currentProgram);
        for (String l : Files.readAllLines(Paths.get(args[0]))) {
            l = l.trim();
            if (l.isEmpty() || l.startsWith("#")) continue;
            String[] p = l.split("\t");
            try { scan(p[0], toAddr(Long.parseLong(p[1].replace("0x", ""), 16))); }
            catch (Exception e) { out.println(p[0] + "\t" + p[1] + "\tERROR\t" + e); }
            out.flush();
        }
        out.close();
    }

    void fixProto(long a, String cc, int nparams) throws Exception {
        Function f = getFunctionAt(toAddr(a));
        if (f == null) return;
        f.setCallingConvention(cc);
        List<ParameterImpl> ps = new ArrayList<>();
        for (int i = 0; i < nparams; i++) ps.add(new ParameterImpl("p" + i, new PointerDataType(), currentProgram));
        f.replaceParameters(ps, Function.FunctionUpdateType.DYNAMIC_STORAGE_ALL_PARAMS, true, SourceType.USER_DEFINED);
    }

    // resolve varnode -> "this+0xNN", "local", "const:N" or "?"
    String resolve(Varnode v, int depth) {
        if (v == null || depth > 12) return "?";
        if (v.isConstant()) return "const:" + v.getOffset();
        if (v.isInput()) {
            if (v.isRegister()) {
                Register r = currentProgram.getRegister(v.getAddress(), v.getSize());
                if (r != null && r.getName().equals("ECX")) return "this+0x0";
                return "reg:" + (r == null ? "?" : r.getName());
            }
            if (v.getAddress().isStackAddress()) return "stackparam";
            return "input";
        }
        PcodeOp d = v.getDef();
        if (d == null) return v.getAddress().isStackAddress() ? "local" : "?";
        int op = d.getOpcode();
        if (op == PcodeOp.COPY || op == PcodeOp.CAST) return resolve(d.getInput(0), depth + 1);
        if (op == PcodeOp.INT_ADD || op == PcodeOp.PTRSUB || op == PcodeOp.PTRADD) {
            Varnode a = d.getInput(0), b = d.getInput(1);
            if (op == PcodeOp.PTRADD) {
                if (b.isConstant() && d.getInput(2).isConstant()) {
                    String base = resolve(a, depth + 1);
                    return addOff(base, b.getOffset() * d.getInput(2).getOffset());
                }
                return "?";
            }
            if (b.isConstant()) {
                if (a.isRegister() && a.isInput()) {
                    Register r = currentProgram.getRegister(a.getAddress(), a.getSize());
                    if (r != null && r.getName().equals("ESP")) return "local";
                }
                String base = resolve(a, depth + 1);
                return addOff(base, b.getOffset());
            }
            if (a.isConstant()) return addOff(resolve(b, depth + 1), a.getOffset());
            return "?";
        }
        if (op == PcodeOp.MULTIEQUAL || op == PcodeOp.INDIRECT) return resolve(d.getInput(0), depth + 1);
        return "?(" + d.getMnemonic() + ")";
    }

    String addOff(String base, long c) {
        c = c & 0xffffffffL;
        if (base.startsWith("this+0x")) {
            long o = Long.parseLong(base.substring(7), 16) + c;
            return "this+0x" + Long.toHexString(o & 0xffffffffL);
        }
        if (base.equals("local") || base.startsWith("local")) return "local";
        return base + "+0x" + Long.toHexString(c);
    }

    static final long VERSION_GLOBAL = 0x01766c28L;
    Map<PcodeBlockBasic, Set<PcodeBlockBasic>> dom = new HashMap<>();

    void computeDom(HighFunction hf) {
        dom.clear();
        List<PcodeBlockBasic> bbs = hf.getBasicBlocks();
        if (bbs.isEmpty()) return;
        Set<PcodeBlockBasic> all = new HashSet<>(bbs);
        PcodeBlockBasic entry = bbs.get(0);
        for (PcodeBlockBasic b : bbs) dom.put(b, b == entry ? new HashSet<>(Collections.singleton(b)) : new HashSet<>(all));
        boolean changed = true;
        while (changed) {
            changed = false;
            for (PcodeBlockBasic b : bbs) {
                if (b == entry) continue;
                Set<PcodeBlockBasic> n = null;
                for (int i = 0; i < b.getInSize(); i++) {
                    PcodeBlockBasic p = (PcodeBlockBasic) b.getIn(i);
                    Set<PcodeBlockBasic> dp = dom.get(p);
                    if (dp == null) continue;
                    if (n == null) n = new HashSet<>(dp); else n.retainAll(dp);
                }
                if (n == null) n = new HashSet<>();
                n.add(b);
                if (!n.equals(dom.get(b))) { dom.put(b, n); changed = true; }
            }
        }
    }

    boolean usesVersion(Varnode v, int depth) {
        if (v == null || depth > 8) return false;
        if (v.getAddress().isMemoryAddress() && v.getAddress().getOffset() == VERSION_GLOBAL) return true;
        PcodeOp d = v.getDef();
        if (d == null) return false;
        if (d.getOpcode() == PcodeOp.LOAD) { Varnode p = d.getInput(1); if (p.isConstant() && p.getOffset() == VERSION_GLOBAL) return true; }
        for (int i = 0; i < d.getNumInputs(); i++) if (usesVersion(d.getInput(i), depth + 1)) return true;
        return false;
    }

    String operand(Varnode v) {
        if (v.isConstant()) return Long.toString(v.getOffset());
        if (usesVersion(v, 0)) return "ver";
        return "x";
    }

    String condText(PcodeOp cb) {
        Varnode c = cb.getInput(1);
        PcodeOp d = c.getDef();
        boolean neg = false;
        while (d != null && d.getOpcode() == PcodeOp.BOOL_NEGATE) { neg = !neg; d = d.getInput(0).getDef(); }
        if (d == null) return "?";
        String sym;
        switch (d.getOpcode()) {
            case PcodeOp.INT_EQUAL: sym = "=="; break;
            case PcodeOp.INT_NOTEQUAL: sym = "!="; break;
            case PcodeOp.INT_LESS: case PcodeOp.INT_SLESS: sym = "<"; break;
            case PcodeOp.INT_LESSEQUAL: case PcodeOp.INT_SLESSEQUAL: sym = "<="; break;
            default: sym = d.getMnemonic();
        }
        String t = operand(d.getInput(0)) + sym + (d.getNumInputs() > 1 ? operand(d.getInput(1)) : "");
        return neg ? "!(" + t + ")" : t;
    }

    String guards(PcodeOpAST op) {
        PcodeBlockBasic b = op.getParent();
        Set<PcodeBlockBasic> mydom = dom.get(b);
        if (mydom == null) return "";
        StringBuilder sb = new StringBuilder();
        for (PcodeBlockBasic cb : dom.keySet()) {
            PcodeOp last = cb.getLastOp();
            if (last == null || last.getOpcode() != PcodeOp.CBRANCH) continue;
            if (!usesVersion(last.getInput(1), 0)) continue;
            PcodeBlockBasic t = (PcodeBlockBasic) cb.getTrueOut();
            PcodeBlockBasic f = (PcodeBlockBasic) cb.getFalseOut();
            String c = condText(last);
            if (t != null && t.getInSize() == 1 && mydom.contains(t) && (f == null || !mydom.contains(f))) sb.append("[" + c + "]");
            else if (f != null && f.getInSize() == 1 && mydom.contains(f) && (t == null || !mydom.contains(t))) sb.append("[!(" + c + ")]");
        }
        return sb.toString();
    }

    void scan(String table, Address a) {
        Function f = getFunctionAt(a);
        if (f == null) { out.println(table + "\t" + a + "\tERROR\tno function"); return; }
        DecompileResults r = di.decompileFunction(f, 120, monitor);
        HighFunction hf = r.getHighFunction();
        if (hf == null) { out.println(table + "\t" + a + "\tERROR\tdecompile failed"); return; }
        computeDom(hf);
        List<PcodeOpAST> calls = new ArrayList<>();
        Iterator<PcodeOpAST> it = hf.getPcodeOps();
        while (it.hasNext()) { PcodeOpAST op = it.next(); if (op.getOpcode() == PcodeOp.CALL) calls.add(op); }
        calls.sort(Comparator.comparing((PcodeOpAST o) -> o.getSeqnum().getTarget()).thenComparingInt(o -> o.getSeqnum().getTime()));
        int idx = 0;
        for (PcodeOpAST op : calls) {
            long tgt = op.getInput(0).getAddress().getOffset();
            String prim, dst = "?", size = "";
            if (tgt == READ_STRING) { prim = "string"; dst = op.getNumInputs() > 2 ? resolve(op.getInput(2), 0) : "?"; size = "u16len+utf16"; }
            else if (tgt == READ_RAW || tgt == READ_BYTE || tgt == READ_U16) {
                prim = tgt == READ_RAW ? "raw" : tgt == READ_BYTE ? "byte" : "u16";
                dst = op.getNumInputs() > 2 ? resolve(op.getInput(2), 0) : "?";
                size = op.getNumInputs() > 3 ? resolve(op.getInput(3), 0).replace("const:", "") : "?";
            } else if (tgt == OPERATOR_ARROW) continue;
            else {
                // other calls: report if an argument points into the builder (nested reader / list)
                StringBuilder sb = new StringBuilder();
                for (int i = 1; i < op.getNumInputs(); i++) { String s = resolve(op.getInput(i), 0); if (s.startsWith("this+")) sb.append(s).append(' '); }
                if (sb.length() == 0) continue;
                Function cf = getFunctionAt(op.getInput(0).getAddress());
                prim = "call:" + (cf == null ? op.getInput(0).getAddress().toString() : cf.getName());
                dst = sb.toString().trim();
            }
            out.println(table + "\t" + a + "\t" + (idx++) + "\t" + op.getSeqnum().getTarget() + "\t" + prim + "\t" + dst + "\t" + size + "\t" + guards(op));
        }
    }
}
