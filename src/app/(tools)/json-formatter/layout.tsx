import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("json-formatter");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
