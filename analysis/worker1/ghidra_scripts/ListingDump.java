// Dump disassembly for address ranges. args: <ranges_file: "start end" hex per line> <out>
//@category NapoleonRE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import java.io.*;
import java.nio.file.*;

public class ListingDump extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        PrintWriter out = new PrintWriter(new FileWriter(a[1]));
        for (String l : Files.readAllLines(Paths.get(a[0]))) {
            String[] p = l.trim().split("\s+");
            if (p.length < 2) continue;
            Address s = toAddr(Long.parseLong(p[0], 16)), e = toAddr(Long.parseLong(p[1], 16));
            out.println("==== " + s + " - " + e);
            InstructionIterator it = currentProgram.getListing().getInstructions(new AddressSet(s, e), true);
            while (it.hasNext()) { Instruction i = it.next(); out.println(i.getAddress() + "  " + i.toString()); }
        }
        out.close();
    }
}
