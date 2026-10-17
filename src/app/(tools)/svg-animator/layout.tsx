import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("svg-animator");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
