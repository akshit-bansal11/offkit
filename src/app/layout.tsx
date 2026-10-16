import { Analytics } from "@vercel/analytics/next";
import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { Observability } from "@/components/site/Observability";
import { siteConfig } from "@/config/site";
import { IS_DESKTOP } from "@/config/target";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  metadataBase: new URL(siteConfig.url),
  title: {
    default: "open-tools",
    template: "%s | open-tools",
  },
  description: siteConfig.description,
  icons: {
    icon: "/favicon.ico",
  },
  keywords: [
    "open tools",
    "online file tools",
    "desktop file tools",
    "image converter",
    "video converter",
    "pdf tools",
    "offline utilities",
  ],
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className="dark">
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        {children}
        {/* The desktop app sends nothing anywhere; analytics and error reporting are for the website only. */}
        {!IS_DESKTOP && (
          <>
            <Analytics />
            <Observability />
          </>
        )}
      </body>
    </html>
  );
}
