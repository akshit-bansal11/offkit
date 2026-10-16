import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("archiver");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
