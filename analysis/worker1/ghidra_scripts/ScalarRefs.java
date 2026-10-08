// Find instructions using given scalar constants and decompile the containing functions.
// Usage (headless postScript args): <comma-separated hex scalars> <output_file> [maxLines]
// Output is Ghidra reconstructed pseudocode - NOT original source.
//@category NapoleonRE
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.scalar.Scalar;
import java.io.*;
import java.util.*;

public class ScalarRefs extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Set<Long> wanted = new HashSet<>();
        for (String s : args[0].split("[+,]")) wanted.add(Long.parseLong(s.trim().replace("0x", ""), 16));
        int maxLines = args.length > 2 ? Integer.parseInt(args[2]) : 400;
        PrintWriter out = new PrintWriter(new FileWriter(args[1], false));
        out.println("// Ghidra reconstructed pseudocode (NOT original source).");
        DecompInterface di = new DecompInterface();
        di.setOptions(new DecompileOptions());
        di.openProgram(currentProgram);
        Map<Function, List<String>> hits = new LinkedHashMap<>();
        InstructionIterator it = currentProgram.getListing().getInstructions(true);
        while (it.hasNext() && !monitor.isCancelled()) {
            Instruction ins = it.next();
            for (int i = 0; i < ins.getNumOperands(); i++) {
                for (Object o : ins.getOpObjects(i)) {
                    if (o instanceof Scalar) {
                        long v = ((Scalar) o).getUnsignedValue();
                        if (wanted.contains(v)) {
                            Function f = getFunctionContaining(ins.getAddress());
                            out.println("// hit 0x" + Long.toHexString(v) + " at " + ins.getAddress() + " in " + (f == null ? "?" : f.getName() + "@" + f.getEntryPoint()) + " : " + ins);
                            if (f != null) hits.computeIfAbsent(f, k -> new ArrayList<>()).add(ins.getAddress().toString());
                        }
                    }
                }
            }
        }
        int n = 0;
        for (Function f : hits.keySet()) {
            if (n++ > 25) break;
            out.println("\n// ===== " + f.getName() + " @" + f.getEntryPoint() + " size=" + f.getBody().getNumAddresses());
            StringBuilder sb = new StringBuilder("// callers: ");
            for (Function c : f.getCallingFunctions(monitor)) sb.append(c.getEntryPoint()).append(' ');
            out.println(sb);
            DecompileResults r = di.decompileFunction(f, 120, monitor);
            if (r == null || !r.decompileCompleted()) { out.println("// decompile failed"); continue; }
            String[] ls = r.getDecompiledFunction().getC().split("\n");
            for (int i = 0; i < Math.min(ls.length, maxLines); i++) out.println(ls[i]);
        }
        out.close();
    }
}
