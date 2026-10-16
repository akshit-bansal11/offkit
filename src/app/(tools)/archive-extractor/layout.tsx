import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("archive-extractor");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
