import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("pdf-merger");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
