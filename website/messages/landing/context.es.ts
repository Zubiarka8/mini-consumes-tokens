import type { contextEn } from "./context.en";

export const contextEs: Record<keyof typeof contextEn, string> = {
  demoBadge: "Ejemplo ilustrativo",
  demoTitle: "Una consulta, menos contexto.",
  demoQuery: "Localiza calculateTotal y su dependencia taxRate.",
  demoView: "Vista del código",
  demoFull: "Archivo completo",
  demoRelevant: "Contexto relevante",
  demoCode: "Código fuente de la factura",
  demoLines: "líneas de código",
  demoFullNote:
    "Solo las líneas resaltadas responden a esta consulta. Desplázate para ver el archivo completo.",
  demoSelectionNote:
    "La función y su dependencia, con los números de línea originales. El resto queda fuera del contexto.",
  demoDisclaimer:
    "Ejemplo sintético. Las líneas ilustran la selección; no son un ahorro de tokens medido.",
  demoBenefitsTitle: "Por qué ayuda",
  demoBenefitFind: "Encuentra lo importante sin leerlo todo",
  demoBenefitSave: "Ahorra tiempo y coste",
  demoBenefitFocus: "Tu asistente se confunde menos",
  demoBenefitSource: "Muestra de dónde sale cada respuesta",
  demoMcpWhat:
    "MCP es una forma estándar de que un asistente de IA pida ayuda a una herramienta, como un enchufe que sirve para cualquier asistente.",
  demoMcpWithout: "Sin MCP",
  demoMcpWithoutText:
    "El asistente abre muchos archivos y adivina cuáles importan.",
  demoMcpWith: "Con MCP",
  demoMcpWithText: "El asistente pide solo la parte que necesita y la recibe.",
};
