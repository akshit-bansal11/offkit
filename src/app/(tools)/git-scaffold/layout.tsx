import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("git-scaffold");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
