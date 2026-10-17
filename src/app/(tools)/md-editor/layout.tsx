import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("md-editor");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
