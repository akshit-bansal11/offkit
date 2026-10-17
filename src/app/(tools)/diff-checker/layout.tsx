import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("diff-checker");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
