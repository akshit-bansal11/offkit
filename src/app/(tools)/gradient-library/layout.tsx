import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("gradient-library");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
