import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("pdf-toolkit");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
