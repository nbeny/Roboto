import type { Metadata } from "next";
import { IBM_Plex_Mono, IBM_Plex_Sans_Condensed } from "next/font/google";

import "./globals.css";

// IBM Plex : une famille dessinee pour l'industrie et l'instrumentation. Le choix n'est
// pas nostalgique, il est fonctionnel — chiffres tabulaires lisibles, condense qui tient
// dans une etiquette etroite, et un caractere qui n'evoque pas une application de gestion.
const mono = IBM_Plex_Mono({
  subsets: ["latin"],
  weight: ["400", "500", "600"],
  variable: "--font-mono",
});

const condensed = IBM_Plex_Sans_Condensed({
  subsets: ["latin"],
  weight: ["500", "600", "700"],
  variable: "--font-condensed",
});

export const metadata: Metadata = {
  title: "Roboto — poste de conduite",
  description: "Supervision du robot mobile autonome Roboto.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="fr" className={`${mono.variable} ${condensed.variable}`}>
      <body>{children}</body>
    </html>
  );
}
