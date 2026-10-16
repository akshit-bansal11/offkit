import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("color-converter");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
