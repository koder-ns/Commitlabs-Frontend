import ProtectedRouteLayout from '@/components/auth/ProtectedRouteLayout'

export default function SettingsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return <ProtectedRouteLayout>{children}</ProtectedRouteLayout>
}
