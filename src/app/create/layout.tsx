import ProtectedRouteLayout from '@/components/auth/ProtectedRouteLayout'

export default function CreateLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return <ProtectedRouteLayout>{children}</ProtectedRouteLayout>
}
