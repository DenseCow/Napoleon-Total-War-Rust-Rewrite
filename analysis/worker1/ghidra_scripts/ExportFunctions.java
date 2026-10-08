// Export the function list: entry, name, size, #callers, #callees, #string refs (first 3 strings).
// Usage: postScript arg <output_tsv>
//@category NapoleonRE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.address.*;
import java.io.*;
import java.util.*;

public class ExportFunctions extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        PrintWriter out = new PrintWriter(new FileWriter(args[0]));
        out.println("entry\tname\tsize\tcallers\tcallees\tnstrings\tstrings");
        Listing lst = currentProgram.getListing();
        ReferenceManager rm = currentProgram.getReferenceManager();
        FunctionIterator it = lst.getFunctions(true);
        int count = 0;
        while (it.hasNext() && !monitor.isCancelled()) {
            Function f = it.next();
            int callers = 0, callees = 0;
            for (Reference r : rm.getReferencesTo(f.getEntryPoint())) if (r.getReferenceType().isCall()) callers++;
            List<String> strs = new ArrayList<>();
            int nstr = 0;
            AddressIterator ai = rm.getReferenceSourceIterator(f.getBody(), true);
            while (ai.hasNext()) {
                Address from = ai.next();
                for (Reference r : rm.getReferencesFrom(from)) {
                    if (r.getReferenceType().isCall()) { callees++; continue; }
                    Data d = lst.getDataAt(r.getToAddress());
                    if (d != null && d.hasStringValue()) {
                        nstr++;
                        if (strs.size() < 3) strs.add(String.valueOf(d.getValue()).replace("\t", " ").replace("\n", " ").replace("\r", " "));
                    }
                }
            }
            out.println(f.getEntryPoint() + "\t" + f.getName() + "\t" + f.getBody().getNumAddresses() + "\t" + callers + "\t" + callees + "\t" + nstr + "\t" + String.join(" | ", strs));
            count++;
        }
        out.close();
        println("exported " + count + " functions");
    }
}
