Pod::Spec.new do |s|
  s.name             = 'dbm_sqlite'
  s.version          = '0.1.0'
  s.summary          = 'Prebuilt SQLite journal for Flutter.'
  s.homepage         = 'https://github.com/lucaswilliameufrasio/dbm-sqlite'
  s.license          = { :type => 'MIT OR Apache-2.0', :file => '../LICENSE' }
  s.author           = { 'dbm-sqlite contributors' => 'https://github.com/lucaswilliameufrasio/dbm-sqlite' }
  s.source           = { :path => '.' }
  s.dependency 'Flutter'
  s.ios.deployment_target = '13.0'
  frameworks = ['Frameworks/DbmSqlite.xcframework']
  if File.directory?(File.join(__dir__, 'Frameworks/DbmSqliteCipher.xcframework'))
    frameworks << 'Frameworks/DbmSqliteCipher.xcframework'
  end
  s.vendored_frameworks = frameworks
  # The XCFramework has arm64 device and arm64 simulator slices only.
  s.pod_target_xcconfig = {
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386 x86_64'
  }
end
