// Validates e-invoices the way a Peppol access point or the KoSIT validator
// does: the document's XSD, then the official Schematron (compiled to XSLT)
// for its syntax. Any failed assertion flagged fatal or error fails the run;
// warnings are printed.
//
// Usage: java -cp Saxon-HE.jar:xmlresolver.jar Validate.java <tools-dir> <file>...

import java.io.File;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import javax.xml.XMLConstants;
import javax.xml.parsers.DocumentBuilderFactory;
import javax.xml.transform.stream.StreamSource;
import javax.xml.validation.SchemaFactory;
import org.w3c.dom.bootstrap.DOMImplementationRegistry;
import org.w3c.dom.ls.DOMImplementationLS;
import org.w3c.dom.ls.LSInput;
import net.sf.saxon.s9api.Processor;
import net.sf.saxon.s9api.XPathCompiler;
import net.sf.saxon.s9api.XdmDestination;
import net.sf.saxon.s9api.XdmItem;
import net.sf.saxon.s9api.XdmNode;
import net.sf.saxon.s9api.XsltExecutable;

public class Validate {
    record Syntax(String name, String xsd, List<String> schematron) {}

    static final String UBL_INV = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2";
    static final String UBL_CN = "urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2";
    static final String CII = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("usage: Validate <tools-dir> <file>...");
            System.exit(2);
        }
        String tools = args[0];
        List<String> peppol = List.of(
            tools + "/peppol/CEN-EN16931-UBL.xslt", tools + "/peppol/PEPPOL-EN16931-UBL.xslt");
        Map<String, Syntax> syntaxes = Map.of(
            UBL_INV, new Syntax("UBL 2.1 Invoice, Peppol BIS 3.0",
                tools + "/ubl/maindoc/UBL-Invoice-2.1.xsd", peppol),
            UBL_CN, new Syntax("UBL 2.1 CreditNote, Peppol BIS 3.0",
                tools + "/ubl/maindoc/UBL-CreditNote-2.1.xsd", peppol),
            CII, new Syntax("CII D16B, EN 16931",
                tools + "/cii/data/standard/CrossIndustryInvoice_100pD16B.xsd",
                List.of(tools + "/en16931/EN16931-CII-validation.xslt")));

        Processor proc = new Processor(false);
        Map<String, XsltExecutable> compiled = new HashMap<>();
        XPathCompiler xp = proc.newXPathCompiler();
        xp.declareNamespace("svrl", "http://purl.oclc.org/dsdl/svrl");
        SchemaFactory sf = SchemaFactory.newInstance(XMLConstants.W3C_XML_SCHEMA_NS_URI);
        // UBL imports these by namespace alone, without a schemaLocation.
        Map<String, String> imports = Map.of(
            "urn:un:unece:uncefact:data:specification:CoreComponentTypeSchemaModule:2",
                tools + "/support/CCTS_CCT_SchemaModule.xsd",
            "http://www.w3.org/2000/09/xmldsig#", tools + "/support/xmldsig-core-schema.xsd",
            "http://uri.etsi.org/01903/v1.3.2#", tools + "/support/XAdES01903v132-201601.xsd",
            "http://uri.etsi.org/01903/v1.4.1#", tools + "/support/XAdES01903v141-201601.xsd");
        DOMImplementationLS ls = (DOMImplementationLS)
            DOMImplementationRegistry.newInstance().getDOMImplementation("LS");
        sf.setResourceResolver((type, namespace, publicId, systemId, baseUri) -> {
            String local = systemId == null ? imports.get(namespace) : null;
            if (local == null) return null;
            LSInput in = ls.createLSInput();
            in.setSystemId(new File(local).toURI().toString());
            return in;
        });

        int failed = 0;
        for (int i = 1; i < args.length; i++) {
            File file = new File(args[i]);
            DocumentBuilderFactory dbf = DocumentBuilderFactory.newInstance();
            dbf.setNamespaceAware(true);
            String ns = dbf.newDocumentBuilder().parse(file).getDocumentElement().getNamespaceURI();
            Syntax syntax = syntaxes.get(ns);
            if (syntax == null) {
                System.out.println("FAIL " + file + ": unknown root namespace " + ns);
                failed++;
                continue;
            }
            List<String> errors = new ArrayList<>();
            List<String> warnings = new ArrayList<>();
            try {
                sf.newSchema(new File(syntax.xsd())).newValidator().validate(new StreamSource(file));
            } catch (org.xml.sax.SAXException e) {
                errors.add("[XSD] " + e.getMessage());
            }
            if (errors.isEmpty()) {
                for (String sch : syntax.schematron()) {
                    XsltExecutable exe = compiled.get(sch);
                    if (exe == null) {
                        exe = proc.newXsltCompiler().compile(new StreamSource(new File(sch)));
                        compiled.put(sch, exe);
                    }
                    XdmDestination out = new XdmDestination();
                    exe.load30().transform(new StreamSource(file), out);
                    XdmNode svrl = out.getXdmNode();
                    for (XdmItem fa : xp.evaluate("//svrl:failed-assert", svrl)) {
                        XdmNode n = (XdmNode) fa;
                        String flag = xp.evaluateSingle("string(@flag)", n).getStringValue();
                        String id = xp.evaluateSingle("string(@id)", n).getStringValue();
                        String text = xp.evaluateSingle("normalize-space(svrl:text)", n).getStringValue();
                        String where = xp.evaluateSingle("string(@location)", n).getStringValue();
                        String line = id + " " + text + " (at " + where + ")";
                        if (flag.equals("warning") || flag.equals("information")) {
                            warnings.add(line);
                        } else {
                            errors.add(line);
                        }
                    }
                }
            }
            String status = errors.isEmpty() ? "ok  " : "FAIL";
            System.out.println(status + " " + file.getName() + " (" + syntax.name() + ")");
            for (String e : errors) System.out.println("     error   " + e);
            for (String w : warnings) System.out.println("     warning " + w);
            if (!errors.isEmpty()) failed++;
        }
        System.out.println((args.length - 1 - failed) + " of " + (args.length - 1) + " documents valid");
        System.exit(failed == 0 ? 0 : 1);
    }
}
