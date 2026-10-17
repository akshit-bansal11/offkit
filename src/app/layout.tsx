import { Analytics } from "@vercel/analytics/next";
import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { Observability } from "@/components/site/Observability";
import { siteConfig } from "@/config/site";

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
    default: "offkit",
    template: "%s | offkit",
  },
  description: siteConfig.description,
  icons: {
    icon: "/favicon.ico",
  },
  keywords: [
    "offkit",
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
        <Analytics />
        <Observability />
      </body>
    </html>
  );
}
