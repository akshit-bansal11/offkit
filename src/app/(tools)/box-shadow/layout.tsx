import { getToolMetadata } from "@/config/tools";

export const metadata = getToolMetadata("box-shadow");

export default function Layout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return children;
}
