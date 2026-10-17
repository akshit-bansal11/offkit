import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("gradient-maker");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
