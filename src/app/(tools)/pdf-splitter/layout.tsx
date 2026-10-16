import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("pdf-splitter");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
