import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("pdf-converter");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
