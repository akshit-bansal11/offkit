import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("frames-extractor");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
