import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("svg-optimizer");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
