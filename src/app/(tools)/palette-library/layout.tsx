import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("palette-library");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
