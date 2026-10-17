import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("convert-from-pdf");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
