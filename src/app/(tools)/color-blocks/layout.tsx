import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("color-blocks");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
